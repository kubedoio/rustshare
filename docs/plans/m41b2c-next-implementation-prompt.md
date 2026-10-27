# Next implementation prompt: M4.1b2c

Implement the proposed [durable replacement specification](../specs/chat-durable-identity-replacement-m41b2c.md)
for kubedoio/rustshare, tracking #424 and #215. First verify the current accepted
Buzz source/image/digest and the [qualification gates](../implementation/buzz-v1alpha2-release-qualification.md).
If upstream acceptance is still blocked, implement/test behind the disabled
boundary and explicitly retain that release blocker.

Start from the exact accepted Elembra main and inspect AGENTS.md. Read the
identity replacement contract, v1alpha2 contract, chat identity storage,
membership verification service/gateway and existing Chat bridge/outbox. Keep
one operation, five specified states, stable conditional command IDs, one-use
fresh evidence, tenant status/audit and atomic evidence/transition acceptance.
The existing verification helper commits independently: refactor the shared
acceptance seam without losing revision fences or pin-rotation protection.
Existing bridge delivery is not proof of remote membership.

Before editing identify packages, callers, event registrations, migration,
SQLx metadata, fixtures and smallest security tests. Implement the complete
worker/storage flow and all crash, retry, concurrency, trust-boundary and live
tests specified above. Reuse the existing outbox; add no queue framework or
speculative options. Keep private human keys exclusively client-side. Update
contract/changelog and provide a short security note and reproducible evidence.

Keep replacement/pairing UX disabled and preserve HTTP/storage guards until a
separately approved enablement change. Do not merge, deploy, change the
production image pin, opt in existing communities, weaken protection, or
restore legacy writers. Make signed-off commits and a draft PR; request fresh
independent human security review. Report exact SHAs/checks and any remaining
release limitations, without equating Elembra tests with Buzz qualification.
