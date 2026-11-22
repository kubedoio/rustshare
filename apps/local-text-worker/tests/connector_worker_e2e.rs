use reqwest::{Client, StatusCode};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn spawn_worker(
    instance_id: &str,
    server: &str,
    credential_file: &Path,
    queue: &Path,
    root: &Path,
) -> Worker {
    Worker(
        Command::new(env!("CARGO_BIN_EXE_elembra-local-text-worker"))
            .args([
                "--instance-id",
                instance_id,
                "--server",
                server,
                "--credential-file",
            ])
            .arg(credential_file)
            .arg("--queue")
            .arg(queue)
            .arg("--root")
            .arg(root)
            .args(["--reconcile-seconds", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start independent Local Text worker process"),
    )
}

fn save_credential(path: &Path, credential: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .expect("create worker credential file");
    file.write_all(credential.as_bytes())
        .expect("write worker credential file");
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .expect("restrict worker credential file");
}

fn queue_pending(queue: &Path, instance_id: &str) -> i64 {
    Connection::open(queue)
        .expect("open worker queue")
        .query_row(
            "SELECT count(*) FROM local_text_queue WHERE instance_id = ?1 AND acknowledged = 0",
            [instance_id],
            |row| row.get(0),
        )
        .expect("read pending queue count")
}

fn revision(content: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(content)))
}

async fn imported_files(
    client: &Client,
    server: &str,
    owner_token: &str,
    scope: &str,
) -> Vec<Value> {
    let response = client
        .get(format!("{server}/api/v1/files"))
        .bearer_auth(owner_token)
        .send()
        .await
        .expect("request Files list");
    assert_eq!(response.status(), StatusCode::OK);
    response
        .json::<Vec<Value>>()
        .await
        .expect("decode Files list")
        .into_iter()
        .filter(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.starts_with(&format!("/{scope}/")))
        })
        .collect()
}

async fn wait_for_versions(
    client: &Client,
    server: &str,
    owner_token: &str,
    scope: &str,
    expected: &[(&str, i64)],
) -> Vec<Value> {
    for _ in 0..150 {
        let files = imported_files(client, server, owner_token, scope).await;
        let versions: BTreeMap<_, _> = files
            .iter()
            .filter_map(|file| Some((file["path"].as_str()?, file["current_version"].as_i64()?)))
            .collect();
        if expected
            .iter()
            .all(|(path, version)| versions.get(*path).is_some_and(|found| found >= version))
        {
            return files;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for Local Text Files import");
}

async fn connector_events(pool: &sqlx::PgPool, instance_id: &str) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT event_json->'data'->'sourceProvenance' FROM integration_outbox \
         WHERE application_id = 'io.elembra.files' \
           AND event_json->'data'->'sourceProvenance'->>'connector_instance_id' = $1 \
         ORDER BY created_at",
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .expect("read Files publication provenance")
}

async fn connector_mappings(
    pool: &sqlx::PgPool,
    instance_id: Uuid,
) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT external_resource_ref, external_revision, target_resource_ref \
         FROM connector_external_mappings WHERE instance_id = $1 ORDER BY external_revision",
    )
    .bind(instance_id)
    .fetch_all(pool)
    .await
    .expect("read Connector-to-Files mappings")
}

#[tokio::test]
#[ignore = "requires a disposable loopback backend, PostgreSQL, and object store"]
async fn independent_worker_imports_retries_recovers_and_stops_after_revocation() {
    assert_eq!(env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(), Ok("1"));
    assert_eq!(
        env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE").as_deref(),
        Ok("1")
    );
    let server = env::var("RUSTSHARE_CONNECTOR_TEST_SERVER").expect("set test backend URL");
    let server_url = url::Url::parse(&server).expect("test backend URL must be valid");
    assert!(
        matches!(
            server_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "Connector worker E2E is restricted to loopback backends"
    );
    let database_url = env::var("DATABASE_URL").expect("set disposable DATABASE_URL");
    let parsed_database_url = url::Url::parse(&database_url).expect("DATABASE_URL must be valid");
    assert!(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ) && parsed_database_url
            .path()
            .trim_start_matches('/')
            .starts_with("rustshare_test"),
        "Connector worker E2E only reads a local rustshare_test database"
    );
    let owner_token = env::var("RUSTSHARE_CONNECTOR_TEST_OWNER_TOKEN")
        .expect("set a disposable test owner's bearer token");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to disposable PostgreSQL");
    let client = Client::new();
    let instance_response = client
        .post(format!("{server}/api/v1/connectors"))
        .bearer_auth(&owner_token)
        .header("Idempotency-Key", Uuid::new_v4().to_string())
        .json(&json!({
            "descriptor": {
                "apiVersion":"elembra.io/v1alpha1",
                "kind":"Connector",
                "metadata":{"id":"io.elembra.connector.local-text","name":"Local Text E2E","version":"0.1.0"},
                "source":{"system":"local-filesystem","authority":"local"},
                "supports":{
                    "operations":{"read":true,"write":false,"delete":false,"list":false,"watch":false,"versions":false,"externalSharingMetadata":false},
                    "modes":{"reference":false,"mirror":false,"import":true,"archive":false,"export":false,"bidirectionalSync":false}
                },
                "publishes":{"resources":[]}
            },
            "sourceConfig":{"destinationFolderId":null},
            "credentialRef":null,
            "mode":"import"
        }))
        .send()
        .await
        .expect("register test Connector");
    assert_eq!(instance_response.status(), StatusCode::CREATED);
    let instance: Value = instance_response.json().await.expect("decode instance");
    let instance_id = instance["id"].as_str().expect("instance id").to_owned();
    let instance_uuid = Uuid::parse_str(&instance_id).expect("valid instance id");

    let credential_response = client
        .post(format!(
            "{server}/api/v1/connectors/{instance_id}/worker-credentials"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("issue worker credential");
    assert_eq!(credential_response.status(), StatusCode::CREATED);
    let credential: Value = credential_response.json().await.expect("decode credential");
    let credential = credential["credential"]
        .as_str()
        .expect("one-time worker credential")
        .to_owned();

    let source = TempDir::new().expect("create source root");
    let scope = format!("connector-{}", Uuid::new_v4());
    let root = source.path();
    let paths = [
        ("people/alex/shared.md", b"alpha v1\n".as_slice()),
        ("people/sam/shared.md", b"sam note\n".as_slice()),
    ];
    for (relative, content) in paths {
        let path = root.join(&scope).join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fs::write(root.join(&scope).join(".env"), b"secret=excluded\n").unwrap();
    fs::write(root.join(&scope).join(".hidden.md"), b"hidden\n").unwrap();
    fs::create_dir_all(root.join(&scope).join("private")).unwrap();
    fs::write(root.join(&scope).join("private/secrets.md"), b"private\n").unwrap();

    let runtime = TempDir::new().expect("create worker state directory");
    let credential_file = runtime.path().join("credential");
    save_credential(&credential_file, &credential);
    let queue = runtime.path().join("queue.sqlite3");
    let first_path = format!("/{scope}/people/alex/shared.md");
    let second_path = format!("/{scope}/people/sam/shared.md");
    let mut worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    let files = wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        &[(&first_path, 1), (&second_path, 1)],
    )
    .await;
    assert_eq!(
        files.len(),
        2,
        "hidden and private source files were excluded"
    );
    assert!(worker.0.try_wait().unwrap().is_none());
    drop(worker);

    let first_v1 = b"alpha v1\n";
    let first_v2 = b"alpha v2\n";
    fs::write(root.join(&scope).join("people/alex/shared.md"), first_v2).unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        &[(&first_path, 2), (&second_path, 1)],
    )
    .await;
    drop(worker);

    let initial_events = connector_events(&pool, &instance_id).await;
    assert_eq!(initial_events.len(), 3);
    for (expected_path, expected_revision) in [
        (format!("{scope}/people/alex/shared.md"), revision(first_v1)),
        (format!("{scope}/people/alex/shared.md"), revision(first_v2)),
        (
            format!("{scope}/people/sam/shared.md"),
            revision(b"sam note\n"),
        ),
    ] {
        assert!(initial_events.iter().any(|event| {
            event["relative_path"] == expected_path
                && event["source_revision"] == expected_revision
                && event["source_resource_ref"].as_str().is_some_and(|value| {
                    value.starts_with("elembra://io.elembra.connector.local-text/file/")
                })
                && event["connector_instance_id"] == instance_id
                && event["observed_modified_at"].is_string()
        }));
    }
    let initial_mappings = connector_mappings(&pool, instance_uuid).await;
    assert_eq!(initial_mappings.len(), 3);
    assert!(initial_mappings.iter().all(|(source, revision, target)| {
        target.starts_with("elembra://io.elembra.files/file/")
            && target.contains(&revision.replace(":", "%3A"))
            && initial_events.iter().any(|event| {
                event["source_resource_ref"].as_str() == Some(source.as_str())
                    && event["source_revision"].as_str() == Some(revision.as_str())
            })
            && files
                .iter()
                .any(|file| target.contains(file["id"].as_str().unwrap()))
    }));

    let first_v2_revision = revision(first_v2);
    Connection::open(&queue)
        .unwrap()
        .execute(
            "UPDATE local_text_queue SET acknowledged = 0 WHERE instance_id = ?1 \
             AND relative_path = ?2 AND revision = ?3",
            rusqlite::params![
                instance_id,
                format!("{scope}/people/alex/shared.md"),
                first_v2_revision
            ],
        )
        .unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    for _ in 0..150 {
        if queue_pending(&queue, &instance_id) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(queue_pending(&queue, &instance_id), 0);
    assert_eq!(connector_mappings(&pool, instance_uuid).await.len(), 3);
    assert_eq!(connector_events(&pool, &instance_id).await.len(), 3);
    drop(worker);

    let first_v3 = b"alpha v3 after offline edit\n";
    fs::write(root.join(&scope).join("people/alex/shared.md"), first_v3).unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    wait_for_versions(
        &client,
        &server,
        &owner_token,
        &scope,
        &[(&first_path, 3), (&second_path, 1)],
    )
    .await;
    drop(worker);

    let rotated = client
        .post(format!(
            "{server}/api/v1/connectors/{instance_id}/worker-credentials"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("rotate worker credential");
    assert_eq!(rotated.status(), StatusCode::CREATED);
    let rotated: Value = rotated.json().await.expect("decode rotated credential");
    let rotated_credential = rotated["credential"].as_str().unwrap();
    let old_session = client
        .get(format!("{server}/api/v1/connectors/worker/session"))
        .header("Authorization", format!("Connector {credential}"))
        .send()
        .await
        .expect("check revoked worker credential");
    assert_eq!(old_session.status(), StatusCode::UNAUTHORIZED);
    let limited = client
        .get(format!("{server}/api/v1/files"))
        .header("Authorization", format!("Connector {rotated_credential}"))
        .send()
        .await
        .expect("check service credential boundary");
    assert_eq!(limited.status(), StatusCode::UNAUTHORIZED);

    let first_v4 = b"alpha v4 after revocation\n";
    fs::write(root.join(&scope).join("people/alex/shared.md"), first_v4).unwrap();
    let worker = spawn_worker(&instance_id, &server, &credential_file, &queue, root);
    tokio::time::sleep(Duration::from_secs(2)).await;
    drop(worker);
    let files = imported_files(&client, &server, &owner_token, &scope).await;
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|file| { file["path"] == first_path && file["current_version"] == 3 }));
    assert_eq!(queue_pending(&queue, &instance_id), 1);
    assert_eq!(connector_mappings(&pool, instance_uuid).await.len(), 4);
    assert_eq!(connector_events(&pool, &instance_id).await.len(), 4);

    let current = client
        .get(format!("{server}/api/v1/connectors/{instance_id}"))
        .bearer_auth(&owner_token)
        .send()
        .await
        .expect("read Connector for cleanup")
        .json::<Value>()
        .await
        .expect("decode Connector");
    client
        .patch(format!("{server}/api/v1/connectors/{instance_id}"))
        .bearer_auth(&owner_token)
        .json(&json!({
            "expectedVersion": current["version"],
            "lifecycleState":"disconnected"
        }))
        .send()
        .await
        .expect("disconnect test Connector")
        .error_for_status()
        .expect("disconnect response");
    pool.close().await;
}
