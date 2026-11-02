#!/usr/bin/env python3
"""Render disposable Kubernetes objects as JSON documents for kubectl apply."""

import argparse
import json
import sys


def obj(kind, name, namespace, *, labels=None, spec=None, data=None):
    metadata = {"name": name}
    if namespace:
        metadata["namespace"] = namespace
    if labels:
        metadata["labels"] = labels
    api_version = "apps/v1" if kind == "Deployment" else "v1"
    result = {"apiVersion": api_version, "kind": kind, "metadata": metadata}
    if spec is not None:
        result["spec"] = spec
    if data is not None:
        result["data"] = data
    return result


def deployment(name, ns, app, containers, *, volumes=None, dns=None):
    pod_spec = {
        "automountServiceAccountToken": False,
        "securityContext": {"seccompProfile": {"type": "RuntimeDefault"}},
        "containers": containers,
    }
    if volumes:
        pod_spec["volumes"] = volumes
    if dns:
        pod_spec["dnsPolicy"] = "None"
        pod_spec["dnsConfig"] = {
            "nameservers": [dns],
            "options": [{"name": "ndots", "value": "1"}, {"name": "timeout", "value": "2"}, {"name": "attempts", "value": "2"}],
        }
    labels = {"app": app}
    return obj("Deployment", name, ns, labels=labels, spec={
        "replicas": 1,
        "selector": {"matchLabels": labels},
        "template": {"metadata": {"labels": labels}, "spec": pod_spec},
    })


def service(name, ns, app, port, target, protocol="TCP"):
    return obj("Service", name, ns, spec={
        "type": "ClusterIP",
        "selector": {"app": app},
        "ports": [{"name": "port-" + str(port), "port": port, "targetPort": target, "protocol": protocol}],
    })


def cnp(name, ns, app, egress):
    return {
        "apiVersion": "cilium.io/v2",
        "kind": "CiliumNetworkPolicy",
        "metadata": {"name": name, "namespace": ns},
        "spec": {"endpointSelector": {"matchLabels": {"app": app}}, "egress": egress},
    }


def dns_egress():
    return {"toEndpoints": [{"matchLabels": {"app": "controlled-dns"}}], "toPorts": [{"ports": [{"port": "53", "protocol": "UDP"}, {"port": "53", "protocol": "TCP"}]}]}


def cluster_dns_egress():
    return {"toEndpoints": [{"matchLabels": {"k8s:io.kubernetes.pod.namespace": "kube-system", "k8s:k8s-app": "kube-dns"}}], "toPorts": [{"ports": [{"port": "53", "protocol": "UDP"}, {"port": "53", "protocol": "TCP"}]}]}


def port_egress(app, port):
    return {"toEndpoints": [{"matchLabels": {"app": app}}], "toPorts": [{"ports": [{"port": str(port), "protocol": "TCP"}]}]}


def render_base(args):
    ns = args.namespace
    yield obj("Namespace", ns, None, labels={"app.kubernetes.io/part-of": "elembra-private-relay-conformance"})
    # Kubernetes NetworkPolicy makes every pod in this fixture namespace
    # egress-deny by default. The Cilium policies below add only named paths.
    yield {
        "apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy",
        "metadata": {"name": "default-deny-egress", "namespace": ns},
        "spec": {"podSelector": {}, "policyTypes": ["Egress"], "egress": []},
    }
    yield {"apiVersion": "v1", "kind": "Service", "metadata": {"name": "controlled-dns", "namespace": ns}, "spec": {
        "type": "ClusterIP", "selector": {"app": "controlled-dns"},
        "ports": [{"name": "dns-udp", "port": 53, "targetPort": 53, "protocol": "UDP"}, {"name": "dns-tcp", "port": 53, "targetPort": 53, "protocol": "TCP"}],
    }}

    for tenant in ("a", "b"):
        upper = tenant.upper()
        owner = "buzz-" + tenant
        for component, port in (("postgres", 5432), ("redis", 6379), ("silo", 9000)):
            app = f"{component}-{tenant}"
            service_name = f"{component}-{tenant}"
            yield service(service_name, ns, app, port, port)
            if component == "postgres":
                container = {
                    "name": app, "image": args.postgres_image, "imagePullPolicy": "IfNotPresent",
                    "envFrom": [{"secretRef": {"name": owner + "-config"}}],
                    "env": [{"name": "POSTGRES_DB", "value": "buzz"}, {"name": "POSTGRES_USER", "value": "buzz"}, {"name": "PGDATA", "value": "/var/lib/postgresql/data/pgdata"}],
                    "ports": [{"name": "postgres", "containerPort": 5432}],
                    "readinessProbe": {"exec": {"command": ["sh", "-ec", "pg_isready -U buzz -d buzz"]}, "initialDelaySeconds": 3, "periodSeconds": 3},
                    "volumeMounts": [{"name": "data", "mountPath": "/var/lib/postgresql/data"}],
                    "resources": {"requests": {"cpu": "100m", "memory": "128Mi"}, "limits": {"cpu": "1", "memory": "1Gi"}},
                }
                yield deployment(app, ns, app, [container], volumes=[{"name": "data", "emptyDir": {}}])
            elif component == "redis":
                container = {
                    "name": app, "image": args.redis_image, "imagePullPolicy": "IfNotPresent",
                    "envFrom": [{"secretRef": {"name": owner + "-config"}}],
                    "args": ["redis-server", "--appendonly", "yes", "--requirepass", "$(REDIS_PASSWORD)"],
                    "ports": [{"name": "redis", "containerPort": 6379}],
                    "readinessProbe": {"exec": {"command": ["/bin/sh", "-ec", "redis-cli -a \"$REDIS_PASSWORD\" ping | grep -q PONG"]}, "initialDelaySeconds": 3, "periodSeconds": 3},
                    "volumeMounts": [{"name": "data", "mountPath": "/data"}],
                    "resources": {"requests": {"cpu": "50m", "memory": "64Mi"}, "limits": {"cpu": "500m", "memory": "512Mi"}},
                }
                yield deployment(app, ns, app, [container], volumes=[{"name": "data", "emptyDir": {}}])
            else:
                container = {
                    "name": app, "image": args.silo_image, "imagePullPolicy": "IfNotPresent",
                    "args": ["server", "/data", "--console-address", ":9001"],
                    "env": [
                        {"name": "MINIO_ROOT_USER", "valueFrom": {"secretKeyRef": {"name": owner + "-config", "key": "BUZZ_S3_ACCESS_KEY"}}},
                        {"name": "MINIO_ROOT_PASSWORD", "valueFrom": {"secretKeyRef": {"name": owner + "-config", "key": "BUZZ_S3_SECRET_KEY"}}},
                    ],
                    "ports": [{"name": "s3", "containerPort": 9000}],
                    "readinessProbe": {"httpGet": {"path": "/minio/health/live", "port": 9000}, "initialDelaySeconds": 3, "periodSeconds": 3},
                    "volumeMounts": [{"name": "data", "mountPath": "/data"}],
                    "resources": {"requests": {"cpu": "50m", "memory": "64Mi"}, "limits": {"cpu": "500m", "memory": "512Mi"}},
                }
                yield deployment(app, ns, app, [container], volumes=[{"name": "data", "emptyDir": {}}])

        # The relay image and behavior are from the exact upstream source pin.
        yield service("buzz-" + tenant, ns, "buzz-" + tenant, 3000, 3000)
        relay = {
            "name": "relay", "image": args.buzz_image, "imagePullPolicy": "IfNotPresent",
            "envFrom": [{"secretRef": {"name": owner + "-config"}}],
            "env": [
                {"name": "BUZZ_BIND_ADDR", "value": "0.0.0.0:3000"},
                {"name": "BUZZ_HEALTH_PORT", "value": "8080"},
                {"name": "BUZZ_METRICS_PORT", "value": "9102"},
                {"name": "DATABASE_URL", "value": f"postgres://buzz:$(POSTGRES_PASSWORD)@postgres-{tenant}:5432/buzz"},
                {"name": "REDIS_URL", "value": f"redis://:$(REDIS_PASSWORD)@redis-{tenant}:6379"},
                {"name": "BUZZ_S3_ENDPOINT", "value": f"http://silo-{tenant}:9000"},
                {"name": "BUZZ_S3_ADDRESSING_STYLE", "value": "path"},
                {"name": "BUZZ_S3_BUCKET", "value": "buzz-media"},
                {"name": "BUZZ_GIT_REPO_PATH", "value": "/data/git"},
                {"name": "BUZZ_AUTO_MIGRATE", "value": "true"},
                {"name": "BUZZ_GIT_CONFORMANCE_PROBE", "value": "true"},
                {"name": "BUZZ_PUSH_ENABLED", "value": "false"},
                {"name": "BUZZ_REQUIRE_AUTH_TOKEN", "value": "true"},
                {"name": "BUZZ_REQUIRE_RELAY_MEMBERSHIP", "value": "true"},
                {"name": "RELAY_URL", "value": f"wss://buzz-edge-{tenant}.{ns}.svc.cluster.local"},
                {"name": "RELAY_OPERATOR_API_ORIGIN", "value": f"https://buzz-edge-{tenant}.{ns}.svc.cluster.local"},
                {"name": "RELAY_OPERATOR_PUBKEYS", "valueFrom": {"secretKeyRef": {"name": owner + "-config", "key": "OPERATOR_PUBKEY"}}},
            ],
            "ports": [{"name": "relay", "containerPort": 3000}, {"name": "health", "containerPort": 8080}],
            "readinessProbe": {"exec": {"command": ["bash", "-ec", "exec 3<>/dev/tcp/127.0.0.1/8080; printf 'GET /_readiness HTTP/1.1\\r\\nHost: localhost\\r\\nConnection: close\\r\\n\\r\\n' >&3; grep -q '200 OK' <&3"]}, "initialDelaySeconds": 10, "periodSeconds": 5, "failureThreshold": 18},
            "volumeMounts": [{"name": "git", "mountPath": "/data/git"}],
            "resources": {"requests": {"cpu": "250m", "memory": "512Mi"}, "limits": {"cpu": "2", "memory": "2Gi"}},
        }
        yield deployment("buzz-" + tenant, ns, "buzz-" + tenant, [relay], volumes=[{"name": "git", "emptyDir": {}}])

        # Initialize the private per-tenant S3 bucket using the source Compose image.
        yield {
            "apiVersion": "batch/v1", "kind": "Job", "metadata": {"name": f"silo-init-{tenant}", "namespace": ns},
            "spec": {"backoffLimit": 3, "template": {"metadata": {"labels": {"app": f"silo-init-{tenant}"}}, "spec": {
                "restartPolicy": "OnFailure", "automountServiceAccountToken": False,
                "containers": [{"name": "mc", "image": args.mc_image, "imagePullPolicy": "IfNotPresent",
                    "env": [
                        {"name": "BUZZ_S3_ACCESS_KEY", "valueFrom": {"secretKeyRef": {"name": owner + "-config", "key": "BUZZ_S3_ACCESS_KEY"}}},
                        {"name": "BUZZ_S3_SECRET_KEY", "valueFrom": {"secretKeyRef": {"name": owner + "-config", "key": "BUZZ_S3_SECRET_KEY"}}},
                    ],
                    "command": ["/bin/sh", "-euc", f"mc alias set local http://silo-{tenant}:9000 \"$BUZZ_S3_ACCESS_KEY\" \"$BUZZ_S3_SECRET_KEY\"; mc mb --ignore-existing local/buzz-media; mc anonymous set none local/buzz-media"],
                    "resources": {"requests": {"cpu": "25m", "memory": "32Mi"}, "limits": {"cpu": "200m", "memory": "128Mi"}},
                }],
            }}},
        }

        yield service(f"buzz-edge-{tenant}", ns, f"buzz-edge-{tenant}", 443, 8443)
        edge = {
            "name": "tls-edge", "image": args.poc_image, "imagePullPolicy": "IfNotPresent",
            "command": ["node", "/opt/cilium/tls-edge.mjs"],
            "env": [
                {"name": "UPSTREAM_HOST", "value": "buzz-" + tenant},
                {"name": "UPSTREAM_PORT", "value": "3000"},
                {"name": "LISTEN_PORT", "value": "8443"},
                {"name": "EXPECTED_HOST", "value": f"buzz-edge-{tenant}.{ns}.svc.cluster.local"},
                {"name": "TLS_CERT_FILE", "value": "/run/cert/tls.crt"},
                {"name": "TLS_KEY_FILE", "value": "/run/cert/tls.key"},
            ],
            "ports": [{"name": "tls", "containerPort": 8443}],
            "readinessProbe": {"tcpSocket": {"port": 8443}, "initialDelaySeconds": 2, "periodSeconds": 2},
            "securityContext": {"runAsNonRoot": True, "runAsUser": 1000, "allowPrivilegeEscalation": False, "readOnlyRootFilesystem": True, "capabilities": {"drop": ["ALL"]}},
            "volumeMounts": [{"name": "tls", "mountPath": "/run/cert", "readOnly": True}],
            "resources": {"requests": {"cpu": "50m", "memory": "32Mi"}, "limits": {"cpu": "500m", "memory": "256Mi"}},
        }
        yield deployment(f"buzz-edge-{tenant}", ns, f"buzz-edge-{tenant}", [edge], volumes=[{"name": "tls", "secret": {"secretName": f"buzz-edge-{tenant}-tls", "defaultMode": 292}}])

    # Connector destinations are deliberately restricted to DNS and both edge
    # endpoints. Both edges stay allowed during DNS poisoning so policy cannot
    # mask an UnsafeDnsTargets rejection.
    yield cnp("connector-egress", ns, "rust-connector", [dns_egress(), port_egress("buzz-edge-a", 8443), port_egress("buzz-edge-b", 8443)])
    yield cnp("seeder-egress", ns, "dm-seeder", [dns_egress(), port_egress("buzz-edge-a", 8443), port_egress("buzz-edge-b", 8443)])
    for tenant in ("a", "b"):
        yield cnp(f"buzz-{tenant}-egress", ns, f"buzz-{tenant}", [cluster_dns_egress(), port_egress(f"postgres-{tenant}", 5432), port_egress(f"redis-{tenant}", 6379), port_egress(f"silo-{tenant}", 9000)])
        yield cnp(f"edge-{tenant}-egress", ns, f"buzz-edge-{tenant}", [cluster_dns_egress(), port_egress(f"buzz-{tenant}", 3000)])
        yield cnp(f"silo-init-{tenant}-egress", ns, f"silo-init-{tenant}", [cluster_dns_egress(), port_egress(f"silo-{tenant}", 9000)])


def render_dns(args):
    ns = args.namespace
    hosts = [
        f"{args.edge_a_ip} buzz-edge-a.{ns}.svc.cluster.local",
        f"{args.edge_b_ip} buzz-edge-b.{ns}.svc.cluster.local",
    ]
    if args.poison_dns:
        hosts.insert(1, f"{args.edge_b_ip} buzz-edge-a.{ns}.svc.cluster.local")
    yield obj("ConfigMap", "controlled-dns", ns, data={
        "Corefile": ".:53 {\n  errors\n  hosts /etc/coredns/hosts {\n    ttl 10\n    reload 1s\n    fallthrough\n  }\n}\n",
        "hosts": "\n".join(hosts) + "\n",
    })
    core = {
        "name": "coredns", "image": args.coredns_image, "imagePullPolicy": "IfNotPresent",
        "args": ["-conf", "/etc/coredns/Corefile"],
        "ports": [{"name": "dns-udp", "containerPort": 53, "protocol": "UDP"}, {"name": "dns-tcp", "containerPort": 53, "protocol": "TCP"}],
        "readinessProbe": {"tcpSocket": {"port": "dns-tcp"}, "initialDelaySeconds": 2, "periodSeconds": 2},
        "volumeMounts": [{"name": "config", "mountPath": "/etc/coredns", "readOnly": True}],
        "resources": {"requests": {"cpu": "25m", "memory": "32Mi"}, "limits": {"cpu": "200m", "memory": "128Mi"}},
    }
    yield deployment("controlled-dns", ns, "controlled-dns", [core], volumes=[{"name": "config", "configMap": {"name": "controlled-dns"}}])


def render_seed(args):
    ns = args.namespace
    yield {
        "apiVersion": "batch/v1", "kind": "Job", "metadata": {"name": "seed-private-dms", "namespace": ns},
        "spec": {"backoffLimit": 0, "template": {"metadata": {"labels": {"app": "dm-seeder"}}, "spec": {
            "restartPolicy": "Never", "automountServiceAccountToken": False,
            "dnsPolicy": "None", "dnsConfig": {"nameservers": [args.dns_ip], "options": [{"name": "ndots", "value": "1"}, {"name": "timeout", "value": "2"}, {"name": "attempts", "value": "2"}]},
            "containers": [{"name": "seed", "image": args.poc_image, "imagePullPolicy": "IfNotPresent", "command": ["node", "/opt/cilium/seed-private-dms.mjs"],
                "envFrom": [{"secretRef": {"name": "buzz-seed-keys"}}, {"configMapRef": {"name": "seed-config"}}],
                "volumeMounts": [{"name": "roots", "mountPath": "/run/roots", "readOnly": True}],
                "securityContext": {"allowPrivilegeEscalation": False, "readOnlyRootFilesystem": True, "capabilities": {"drop": ["ALL"]}},
                "resources": {"requests": {"cpu": "50m", "memory": "64Mi"}, "limits": {"cpu": "500m", "memory": "512Mi"}},
            }],
            "volumes": [{"name": "roots", "secret": {"secretName": "buzz-root-pems", "defaultMode": 292}}],
        }}},
    }


def render_runner(args):
    ns = args.namespace
    yield obj("Pod", "rust-connector", ns, labels={"app": "rust-connector"}, spec={
        "restartPolicy": "Never", "automountServiceAccountToken": False,
        "dnsPolicy": "None", "dnsConfig": {"nameservers": [args.dns_ip], "options": [{"name": "ndots", "value": "1"}, {"name": "timeout", "value": "2"}, {"name": "attempts", "value": "2"}]},
        "containers": [
            {"name": "rust-test", "image": args.runner_image, "imagePullPolicy": "IfNotPresent", "command": ["/bin/sh", "-ec", "sleep infinity"],
                "envFrom": [{"secretRef": {"name": "connector-test-credentials"}}, {"configMapRef": {"name": "connector-test-config"}}],
                "volumeMounts": [{"name": "roots", "mountPath": "/run/trust", "readOnly": True}],
                "resources": {"requests": {"cpu": "500m", "memory": "768Mi"}, "limits": {"cpu": "2", "memory": "2Gi"}},
            },
            {"name": "postgres", "image": args.pgvector_image, "imagePullPolicy": "IfNotPresent",
                "envFrom": [{"secretRef": {"name": "connector-test-credentials"}}],
                "env": [{"name": "POSTGRES_DB", "value": "rustshare_test"}, {"name": "POSTGRES_USER", "value": "postgres"}, {"name": "PGDATA", "value": "/var/lib/postgresql/data/pgdata"}],
                "ports": [{"name": "postgres", "containerPort": 5432}],
                "readinessProbe": {"exec": {"command": ["pg_isready", "-U", "postgres", "-d", "rustshare_test"]}, "initialDelaySeconds": 3, "periodSeconds": 3},
                "volumeMounts": [{"name": "pgdata", "mountPath": "/var/lib/postgresql/data"}],
                "resources": {"requests": {"cpu": "200m", "memory": "256Mi"}, "limits": {"cpu": "1", "memory": "1Gi"}},
            },
        ],
        "volumes": [{"name": "roots", "secret": {"secretName": "buzz-root-ders", "defaultMode": 292}}, {"name": "pgdata", "emptyDir": {}}],
    })


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--phase", choices=("base", "dns", "seed", "runner"), required=True)
    parser.add_argument("--namespace", default="elembra-buzz-test")
    parser.add_argument("--buzz-image", default="local/buzz-upstream-cilium:test")
    parser.add_argument("--poc-image", default="local/buzz-cilium-tools:test")
    parser.add_argument("--runner-image", default="local/rustshare-cilium-runner:test")
    parser.add_argument("--postgres-image", default="postgres:17-alpine")
    parser.add_argument("--redis-image", default="redis:7-alpine")
    parser.add_argument("--silo-image", default="pgsty/silo:RELEASE.2026-09-16T00-00-00Z")
    parser.add_argument("--mc-image", default="pgsty/mc:RELEASE.2026-09-16T00-00-00Z")
    parser.add_argument("--pgvector-image", default="pgvector/pgvector:pg16")
    parser.add_argument("--coredns-image", default="registry.k8s.io/coredns/coredns:v1.11.3")
    parser.add_argument("--edge-a-ip")
    parser.add_argument("--edge-b-ip")
    parser.add_argument("--dns-ip")
    parser.add_argument("--poison-dns", action="store_true")
    args = parser.parse_args()
    if args.phase == "base":
        objects = render_base(args)
    elif args.phase == "dns":
        if not args.edge_a_ip or not args.edge_b_ip:
            parser.error("dns phase requires --edge-a-ip and --edge-b-ip")
        objects = render_dns(args)
    elif args.phase == "seed":
        if not args.dns_ip:
            parser.error("seed phase requires --dns-ip")
        objects = render_seed(args)
    else:
        if not args.dns_ip:
            parser.error("runner phase requires --dns-ip")
        objects = render_runner(args)
    for index, resource in enumerate(objects):
        if index:
            print("---")
        print(json.dumps(resource, separators=(",", ":")))


if __name__ == "__main__":
    main()
