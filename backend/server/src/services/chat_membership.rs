//! Durable Elembra verification receipts; Buzz membership is read only over HTTP.
use crate::buzz_gateway::{BuzzGatewayClient, BuzzMembershipObservation, BuzzMembershipRequest};
use rustshare_resource_auth::BuzzAuthorityError;
use sqlx::PgPool;

/// Verify one fresh Buzz membership read and durably accept its nonce once.
/// This records an observation, never activates a binding or grants membership.
/// All failures are unavailable/deny. The revision floor also includes prior receipts.
pub async fn verify_buzz_membership(
    pool: &PgPool,
    client: &BuzzGatewayClient,
    relay_url: &str,
    relay_pubkey: &str,
    community_id: uuid::Uuid,
    pubkey: &str,
    minimum_revision: u64,
) -> Result<BuzzMembershipObservation, BuzzAuthorityError> {
    let key = nostr::PublicKey::from_hex(pubkey)
        .map_err(|_| BuzzAuthorityError::Config("invalid membership public key".into()))?;
    if key.to_hex() != pubkey || community_id.is_nil() {
        return Err(BuzzAuthorityError::Config(
            "noncanonical membership identity".into(),
        ));
    }
    let minimum_revision_db = i64::try_from(minimum_revision)
        .map_err(|_| BuzzAuthorityError::Config("membership revision overflow".into()))?;
    let req = BuzzMembershipRequest {
        request_id: uuid::Uuid::new_v4(),
        community_id,
        pubkey,
    };
    sqlx::query(
        "INSERT INTO chat_buzz_membership_verifications \
        (request_id, community_id, pubkey, relay_pubkey, minimum_revision) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(req.request_id)
    .bind(community_id)
    .bind(pubkey)
    .bind(relay_pubkey)
    .bind(minimum_revision_db)
    .execute(pool)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot persist membership request".into()))?;
    let observation = client
        .check_membership(relay_url, relay_pubkey, &req, minimum_revision)
        .await?;
    let mut acceptance = pool
        .begin()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot begin membership acceptance".into()))?;
    // Serialize acceptance for this authority/key, including concurrent reads.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{relay_pubkey}:{community_id}:{pubkey}"))
        .execute(&mut *acceptance)
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot lock membership acceptance".into()))?;
    let accepted = sqlx::query(
        "UPDATE chat_buzz_membership_verifications AS pending \
        SET accepted_at=now(), accepted_revision=$2 WHERE request_id=$1 AND accepted_at IS NULL \
        AND requested_at >= now() - interval '30 seconds' AND NOT EXISTS \
        (SELECT 1 FROM chat_buzz_membership_verifications previous \
         WHERE previous.community_id=pending.community_id AND previous.pubkey=pending.pubkey \
         AND previous.relay_pubkey=pending.relay_pubkey AND previous.accepted_at IS NOT NULL \
         AND previous.accepted_revision > $2)",
    )
    .bind(req.request_id)
    .bind(observation.membership_revision as i64)
    .execute(&mut *acceptance)
    .await
    .map_err(|_| BuzzAuthorityError::Transport("cannot persist membership acceptance".into()))?;
    if accepted.rows_affected() != 1 {
        return Err(BuzzAuthorityError::InvalidResponse(
            "membership request expired or consumed".into(),
        ));
    }
    acceptance
        .commit()
        .await
        .map_err(|_| BuzzAuthorityError::Transport("cannot commit membership acceptance".into()))?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buzz_gateway::BuzzMembership;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use chrono::Utc;
    use nostr::nips::nip98::{verify_auth_header, HttpMethod};
    use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
    use reqwest::Client;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{sync::Arc, time::Duration};
    use url::Url;
    use uuid::Uuid;

    fn test_client(keys: Keys) -> BuzzGatewayClient {
        BuzzGatewayClient::new_for_test(keys, Client::builder()).unwrap()
    }
    fn relay_19030(keys: &Keys, content: Value) -> nostr::Event {
        EventBuilder::new(Kind::Custom(19030), content.to_string())
            .sign_with_keys(keys)
            .unwrap()
    }
    fn auth_header(keys: &Keys, url: &Url, body: &[u8]) -> String {
        let tags = [
            vec!["u".into(), url.to_string()],
            vec!["method".into(), "POST".into()],
            vec!["payload".into(), hex::encode(Sha256::digest(body))],
            vec!["nonce".into(), Uuid::new_v4().to_string()],
        ]
        .into_iter()
        .map(|t| Tag::parse(t).unwrap());
        let event = EventBuilder::new(Kind::HttpAuth, "")
            .tags(tags)
            .sign_with_keys(keys)
            .unwrap();
        format!("Nostr {}", STANDARD.encode(event.as_json()))
    }

    #[tokio::test]
    #[ignore = "requires migrated disposable PostgreSQL"]
    async fn membership_http_persists_single_use_nonce_and_rejects_replay() {
        assert_eq!(
            std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
            Ok("1")
        );
        let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let service = Keys::generate();
        let service_pubkey = service.public_key();
        let relay = Keys::generate();
        let relay_pubkey = relay.public_key().to_hex();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let saved = Arc::new(tokio::sync::Mutex::new(None::<Value>));
        let app = axum::Router::new().route(
            "/api/v1/relay/membership/check",
            axum::routing::post({
                let saved = saved.clone();
                move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let relay = relay.clone();
                    let saved = saved.clone();
                    async move {
                        let url = format!("http://{address}/api/v1/relay/membership/check");
                        let caller = verify_auth_header(
                            headers["authorization"].to_str().unwrap(),
                            &Url::parse(&url).unwrap(),
                            HttpMethod::POST,
                            Timestamp::now(),
                            Some(&body),
                        )
                        .unwrap();
                        assert_eq!(caller, service_pubkey);
                        let mut saved = saved.lock().await;
                        if let Some(old) = saved.as_ref() {
                            return axum::Json(old.clone());
                        }
                        let mut content: Value = serde_json::from_slice(&body).unwrap();
                        content["membership"] = json!("active");
                        content["membership_revision"] = json!(1);
                        content["evaluated_at"] = json!(Utc::now().timestamp());
                        let response = serde_json::to_value(relay_19030(&relay, content)).unwrap();
                        *saved = Some(response.clone());
                        axum::Json(response)
                    }
                }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = test_client(service);
        let community = Uuid::new_v4();
        let key = Keys::generate().public_key().to_hex();
        let url = format!("ws://{address}");
        let observation =
            verify_buzz_membership(&pool, &client, &url, &relay_pubkey, community, &key, 0)
                .await
                .unwrap();
        assert_eq!(observation.membership, BuzzMembership::Active);
        let accepted:bool=sqlx::query_scalar("SELECT accepted_at IS NOT NULL FROM chat_buzz_membership_verifications WHERE request_id=$1")
            .bind(Uuid::parse_str(&observation.request_id).unwrap()).fetch_one(&pool).await.unwrap();
        assert!(accepted);
        assert_eq!(sqlx::query("UPDATE chat_buzz_membership_verifications SET accepted_at=now() WHERE request_id=$1 AND accepted_at IS NULL")
            .bind(Uuid::parse_str(&observation.request_id).unwrap()).execute(&pool).await.unwrap().rows_affected(),0);
        assert!(
            verify_buzz_membership(&pool, &client, &url, &relay_pubkey, community, &key, 0)
                .await
                .is_err()
        );
        let counts:(i64,i64)=sqlx::query_as("SELECT count(*),count(accepted_at) FROM chat_buzz_membership_verifications WHERE community_id=$1")
            .bind(community).fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (2, 1));
        sqlx::query("INSERT INTO chat_buzz_membership_verifications \
            (request_id,community_id,pubkey,relay_pubkey,minimum_revision,accepted_at,accepted_revision) \
            VALUES ($1,$2,$3,$4,0,now(),2)")
            .bind(Uuid::new_v4()).bind(community).bind(&key).bind(&relay_pubkey).execute(&pool).await.unwrap();
        *saved.lock().await = None;
        // A fresh, correctly signed lower revision is rejected by the durable floor.
        assert!(
            verify_buzz_membership(&pool, &client, &url, &relay_pubkey, community, &key, 0)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM chat_buzz_membership_verifications WHERE community_id=$1")
            .bind(community)
            .execute(&pool)
            .await
            .unwrap();
        server.abort();
    }

    #[tokio::test]
    #[ignore = "requires disposable Elembra/Buzz databases, Redis and built Buzz binary; waits for response expiry"]
    async fn membership_real_buzz_client_and_stale_replay() {
        assert_eq!(
            std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
            Ok("1")
        );
        let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
            .await
            .unwrap();
        let buzz_admin_url = std::env::var("BUZZ_TEST_DATABASE_URL").unwrap();
        let admin = sqlx::PgPool::connect(&buzz_admin_url).await.unwrap();
        let name = format!("membership_client_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
        let mut database_url = Url::parse(&buzz_admin_url).unwrap();
        database_url.set_path(&name);
        assert!(matches!(
            database_url.host_str(),
            Some("127.0.0.1" | "localhost")
        ));
        let service = Keys::generate();
        let relay = Keys::generate();
        let pin = relay.public_key().to_hex();
        let relay_url = "ws://localhost:55447";
        struct Process(std::process::Child);
        impl Drop for Process {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let log =
            std::fs::File::create(std::env::temp_dir().join("elembra-membership-live-relay.log"))
                .unwrap();
        let process = Process(
            std::process::Command::new(std::env::var("BUZZ_TEST_RELAY_BINARY").unwrap())
                .env("DATABASE_URL", database_url.as_str())
                .env("REDIS_URL", std::env::var("BUZZ_TEST_REDIS_URL").unwrap())
                .env("RELAY_URL", relay_url)
                .env("BUZZ_BIND_ADDR", "127.0.0.1:55447")
                .env("BUZZ_HEALTH_PORT", "55448")
                .env("BUZZ_METRICS_PORT", "55449")
                .env("BUZZ_AUTO_MIGRATE", "true")
                .env("BUZZ_GIT_ENABLED", "false")
                .env("BUZZ_RELAY_PRIVATE_KEY", relay.secret_key().to_secret_hex())
                .env("RELAY_OWNER_PUBKEY", service.public_key().to_hex())
                .env(
                    "RELAY_TRUSTED_SERVICE_PUBKEYS",
                    service.public_key().to_hex(),
                )
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let http = Client::new();
        let mut ready = false;
        for _ in 0..120 {
            if http
                .get("http://localhost:55447/health")
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        assert!(ready, "actual Buzz relay did not start");
        let gateway = test_client(service.clone());
        let identity = gateway.community_identity(relay_url).await.unwrap();
        let community = Uuid::parse_str(&identity.community_id).unwrap();
        assert_eq!(identity.relay_pubkey, pin);
        // Test-only deployment configuration, never a production membership read.
        let buzz = sqlx::PgPool::connect(database_url.as_str()).await.unwrap();
        sqlx::query("UPDATE communities SET membership_v1alpha2=true WHERE id=$1")
            .bind(community)
            .execute(&buzz)
            .await
            .unwrap();
        let key = Keys::generate().public_key().to_hex();
        assert_eq!(
            verify_buzz_membership(&pool, &gateway, relay_url, &pin, community, &key, 0)
                .await
                .unwrap()
                .membership,
            BuzzMembership::Absent
        );
        assert_eq!(
            verify_buzz_membership(
                &pool,
                &gateway,
                relay_url,
                &pin,
                community,
                &service.public_key().to_hex(),
                0
            )
            .await
            .unwrap()
            .membership,
            BuzzMembership::Active
        );
        let req = BuzzMembershipRequest {
            request_id: Uuid::new_v4(),
            community_id: community,
            pubkey: &key,
        };
        let url = Url::parse("http://localhost:55447/api/v1/relay/membership/check").unwrap();
        let body = serde_json::to_vec(&req).unwrap();
        let header = auth_header(&service, &url, &body);
        let raw: Value = http
            .post(url)
            .header("Authorization", header)
            .body(body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(gateway.membership_from_19030(&raw, &pin, &req, 0).is_ok());
        let replay = BuzzMembershipRequest {
            request_id: Uuid::new_v4(),
            ..req
        };
        assert!(gateway
            .membership_from_19030(&raw, &pin, &replay, 0)
            .is_err());
        tokio::time::sleep(Duration::from_secs(31)).await;
        assert!(gateway.membership_from_19030(&raw, &pin, &req, 0).is_err());
        sqlx::query("DELETE FROM chat_buzz_membership_verifications WHERE community_id=$1")
            .bind(community)
            .execute(&pool)
            .await
            .unwrap();
        buzz.close().await;
        drop(process);
        sqlx::query(&format!("DROP DATABASE {name}"))
            .execute(&admin)
            .await
            .unwrap();
    }
}
