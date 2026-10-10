# Comms API surface — spear · dir-sim · stalwart, with Quack as the database

READ BY: anyone starting D-MAIL-*, D-SVC-1, D-FLOW-5, D-DOC-*, D-IAM-* or
`hubspo-comms` / `hubspo-store` / `hubspo-query` work.

Measured 2026-10-10 against: spear `29dee09`, stalwart (AdaWorldAPI fork)
`1627d121`, OGAR `d402cf7`, lance-graph `488571c1`. Every row below was read
in source; `file:line` is relative to each repo. Status words:
**CODE** (behaviour is at the cited line), **DOC** (written down only),
**ABSENT** (the search space that came back empty is named).

## 0. Where each concern lives

| concern | owner | how HubSPO-rs reaches it |
|---|---|---|
| storage of CRM / comms rows | **Lance** datasets owned by HubSPO-rs | one dataset per table, one version per publish (H-ADR-15) |
| every read (filter, list, view, count, group, fk join) | **Quack** (`lance-graph-quack`) lowered to **mask-risc** | `hubspo-query`; HubSPO-rs implements Quack's `Registrar` + `Binder` (§5) |
| who a person is, which team, which address | **dir-sim** (`lance-graph-dir-sim` over `ogar-dir-sim`) | in process, Rust API (§2) |
| mail placement, mailbox IAM, mail graph, content store | **spear** | in process, Rust API (§1) |
| SMTP / JMAP / DAV / delivery queue | **stalwart**, separate process (AGPL, H-ADR-8) | wire only (§3) |

Quack has no storage and no catalog of its own:
`lance-graph-quack/src/bind.rs` names the `Registrar` seam and says "no crate
owns a table catalog or storage allocation yet … only tests implement it".
"Quack as the database" therefore means HubSPO-rs owns three things:
1. **The catalog.** It implements `Registrar` and `Binder`.
2. **Storage.** The Lance datasets.
3. **Residency.** It loads each column once as an Arrow buffer, and every
   query borrows it.

The fourth piece already exists: Quack + mask-risc evaluate every query.
P-QDB-1 (§5) proves this end to end.

## 1. spear (in process)

| capability | API | evidence | status |
|---|---|---|---|
| parse RFC 5322 (stalwart's own parser, `mail-parser`) | `mail::ingest::parse(&[u8]) -> Result<ParsedMail>`; id = first 16 B of SHA-256 | `src/mail/ingest.rs:232` | CODE |
| place by SMTP envelope via dir-sim | `MailDirectory::deliver(&mut self, &ParsedMail, envelope:&[&str], ms) -> (Vec<MessageRow>, Vec<Delivery>)` | `src/mail/directory.rs:247` | CODE |
| outcome per recipient | `Delivery::{Placed, EmptyGroup, AlreadyPlaced, NotARecipient, Unknown, Contested, Rejected}` | `src/mail/directory.rs:65-93` | CODE |
| group → mailbox members (transitive, cycle-safe) | `MailDirectory::mailbox_members(&Guid128) -> Vec<Guid128>` | `src/mail/directory.rs:191` | CODE |
| store body + append rows | `Spear::deliver(&self, &mut MailDirectory, raw, envelope, ms)` | `src/mail/store.rs:443` | CODE |
| inbox id | `inbox_id(owner) = SHA-256("spear:inbox:"‖guid)[..16]` | `src/mail/directory.rs:107` | CODE |
| mailbox IAM | `RBAC_PLUG` over `email`, `bind()`, `DirectoryActors::{for_principal, delegated}`, `visible_rows` | `src/mail/iam.rs:395-614` | CODE |
| mail graph (Cypher) | `MailGraph::build(dir, rows)`, `query(cypher)` | `src/mail/graph.rs:66, 141` | CODE |
| content store | `ContentStore::{put, get, exists}` (SHA-256, two-level dirs) | `src/content/mod.rs:179-203` | CODE |
| SharePoint upload | `UploadIntent`, `DriveScope` types only; `transport/` empty | `src/sharepoint/transport/mod.rs:9-17` | DOC |
| send receipts / activity tables | — | grep `send_receipt\|activity` over `src/` hits doc comments only | ABSENT |
| thread id | column declared, always null (`FixedSizeBinary::new_null`) | `src/mail/ingest.rs:359` | ABSENT |

Not for HubSPO-rs reads:
- `query::messages::MessageQuery` is a DataFusion `SessionContext` over a
  `MemTable` (`src/query/messages.rs:15-34`). DataFusion is in its grace
  period, so HubSPO-rs reads spear's `messages` table through Quack instead
  (OD-7).

Conflicts to raise with spear (not fixed here):
- `docs/STALWART_INTEGRATION.md` plans spear as a storage backend linked
  INTO stalwart. That contradicts H-ADR-8 (stalwart stays a separate
  process, never linked).
- `directory.rs:46` says stalwart's dir-sim directory makes "the same
  decision". That directory exists, but nothing can reach it at runtime (§3).

## 2. dir-sim (in process)

OGAR owns meaning (`Change`, `Violation`, `ExecutionPlan`, `Guid128`); lance-graph executes it over Quack/mask-risc.
- **Layout.** `lance-graph-dir-sim` is its own workspace and needs OGAR as a
  sibling checkout.
- **No I/O.** It performs no I/O of any kind (`lance-graph-dir-sim/src/lib.rs:7-25`).
- **No feature flags.**

| need | API | evidence | status |
|---|---|---|---|
| membership | `View::is_member(&user, &group) -> bool` | `lance-graph-dir-sim/src/view.rs:447` | CODE |
| group headcount per user | `rule::member_counts(&View, &group) -> Vec<i64>` | `src/rule.rs:30` | CODE |
| text → user via Quack (already a `Binder`) | `bind::where_eq(&View, "upn"\|"smtp", literal) -> Result<Kept, _>` | `src/bind.rs:105` | CODE |
| address → holder | `validate::address_owner(&View, KeyId)`; contested = error with every holder | `src/validate.rs:482` | CODE |
| address → recipient (cloud aware) | `address_recipient`, `address_recipient_in(.., &CloudMailboxes)` | `src/validate.rs:512, 530` | CODE |
| literal → key | `Dicts::key_lookup(&str) -> Option<KeyId>` (normalised, never mints) | `src/snapshot.rs:194` | CODE |
| mailbox host | `Deployment::host_of(&Recipient) -> Option<MailboxHost::{ExchangeServer, Stalwart, ExchangeOnline}>` | `ogar-dir-sim/src/deployment.rs:23-109` | CODE |
| versions | `VersionStore::{observe, view, simulate, validate, promote_desired, diff, plan, lineage, tag}` | `src/store.rs:94-285` | CODE, in memory only |
| import | LDIF (`ogar_ad::ldif::parse`), Graph JSON (`ogar_az::ingest_page`), `observe::from_ad` | `ogar-ad/src/ldif.rs:30`, `ogar-az/src/lib.rs:455`, `src/observe.rs:78` | CODE |
| limits | `MAX_USERS = MAX_GROUPS = 65_536` per population (u16 ordinals) | `src/snapshot.rs:62-64` | CODE |

Gaps that matter for HubSPO-rs:
1. **Nested groups are not expanded transitively.** A nested group is held
   by identity (`snapshot.rs:38-41`). spear's `mailbox_members` does the
   closure; HubSPO-rs should reuse that rather than write a second one.
2. **No mailbox delegation.** Send-as, send-on-behalf and FullAccess are
   ABSENT (grep over the 7 dir-sim/rbac crates).
3. **No queue or round-robin primitive** (ABSENT, same grep). Ticket routing
   is D-FLOW-5's job.
4. **No document ACL model.** Only OneDrive id resolution exists
   (`ogar-az/src/mailbox.rs:204`).
5. **No `ActorSource` over a dir-sim `View` in either tree.** spear's
   `DirectoryActors` is the only one.
6. **Versions are not persisted to Lance.** `VersionId` is designed for it
   (`ogar-dir-sim/src/provenance.rs:16`).
7. **Only `upn` and `smtp` are queryable attributes.**

## 3. stalwart (separate process, wire only)

- **Release.** The fork reports v0.16.25 (`CHANGELOG.md:7`).
- **Administration** is not a REST API. It is JMAP `x:{Type}/{get|set|query}`
  under `urn:stalwart:jmap` (`crates/jmap-proto/src/request/method.rs:238, 358`).

### Tickets — inbound to a support address
| step | wire | key fields | evidence | status |
|---|---|---|---|---|
| notify | webhook (`x:WebHook`), body `{"events":[{id, createdAt, type, data}]}`, `X-Signature` = base64 HMAC-SHA256 of the body | events: `message-ingest.ham`, `.spam`, `.imap-append`, `.jmap-append`, `.duplicate`, `.error` | `crates/common/src/telemetry/webhooks/mod.rs:136-144`; `crates/trc/src/event/enums_impl.rs:316-322` | CODE |
| notify payload | `message-ingest.ham` | AccountId, DocumentId, MailboxId, BlobId, ChangeId, MessageId, Size, SpanId — **no ThreadId, no JMAP Email id** | `crates/email/src/message/ingest.rs:696-714` | CODE |
| push alternative | EventSource `/jmap/eventsource`, WebSocket `/jmap/ws`, `PushSubscription/set`; `emailPush` carries from/to/subject/references/bodyValues | `crates/http/src/request.rs:212-219`; `crates/email/src/push/mod.rs:44-49` | CODE |
| pre-acceptance | MTA hook (`x:MtaHook`): stages Connect…Data; request envelope + message; response accept/discard/reject/quarantine + addHeader/addRecipient | `crates/smtp/src/inbound/hooks/mod.rs:15-192` | CODE |
| fetch | `Email/get`, `Thread/get`, `GET /jmap/download/{acct}/{blobId}/{name}` | `method.rs:117, 126`; `request.rs:149` | CODE |

### Mail — send on behalf
- `EmailSubmission/set` needs `emailId` + `identityId`; an envelope `mailFrom` other than the identity's fails `forbiddenFrom` (`crates/jmap/src/submission/set.rs:505-542`). `sendAt`, `undoStatus`, `onSuccessUpdateEmail` exist. CODE.
- On-behalf: master-user impersonation `user%master` with `Impersonate` + `Authenticate` permissions (`crates/common/src/auth/authentication.rs:262-270`). App passwords cannot impersonate; API keys (Bearer `API_…`) have no master path. CODE.
- Threading: thread id from In-Reply-To/References + normalised subject (`crates/email/src/message/ingest.rs:163-188`). CODE.

### Documents
- Blobs: `Blob/{get,copy,upload,lookup}`, `POST /jmap/upload/{acct}`; `maxUploadSize` and `Email.maxAttachmentSize` default 50 MB (`crates/registry/src/schema/structs_impl.rs:23299, 20429`). CODE.
- Files: JMAP `FileNode/*` (`urn:ietf:params:jmap:filenode`) and WebDAV `/dav/file`, limits in `x:FileStorage` (`structs.rs:2912-2918`). CODE.

### Delivery
| signal | evidence | status |
|---|---|---|
| webhook events `delivery.delivered`, `.completed`, `.failed`, `.dsn-success`, `.dsn-temp-fail`, `.dsn-perm-fail`, `queue.message-queued`, `queue.rescheduled` | `crates/trc/src/event/enums_impl.rs:87-120, 402-407` | CODE |
| `delivery.delivered` carries SpanId, Hostname, To, Code, Details, Elapsed; QueueId only on `delivery.attempt-start` → correlate by SpanId | `crates/smtp/src/outbound/session.rs:279-287`; `delivery.rs:59-62` | CODE |
| queue state | `x:QueuedMessage/{get,query,set}`, recipient status Scheduled / Completed / TemporaryFailure / PermanentFailure | `crates/registry/src/schema/structs.rs:4334, 4395` | CODE |
| `EmailSubmission.deliveryStatus` from the live queue entry | `crates/jmap/src/submission/get.rs:150-171` | CODE |
| `mdnBlobIds` / `dsnBlobIds` | always `[]` | `crates/jmap/src/submission/get.rs:248-250` | CODE (empty) |
| `displayed` | always `unknown` | `crates/jmap/src/submission/get.rs:218-221` | CODE (constant) |
| MDN/send, open/click tracking, OAuth `client_credentials` | method table, `crates/**` grep, `crates/http/src/auth/oauth` | ABSENT |

### Fork addition, and why HubSPO-rs does not depend on it
- `crates/directory/src/backend/dirsim/` (`DirSimDirectory`, feature
  `dir-sim`) resolves recipients from lance-graph-dir-sim; `authenticate`
  always errors.
- Nothing outside the module calls `DirSimDirectory::new`, and the registry
  `Directory` enum has only `Ldap | Sql | Oidc`
  (`crates/registry/src/schema/structs.rs:927-931`).
- It cannot be configured, so D-MAIL-1's "no dir-sim backend in stalwart"
  stands as the runtime truth. The module is the place it would land.

## 4. Feature map — what HubSPO-rs builds on which surface

| feature | in (event / data) | stored as (Lance table, Quack lanes) | read through Quack | gaps owned by HubSPO-rs |
|---|---|---|---|---|
| **Tickets** | stalwart `message-ingest.ham` webhook → `Email/get` by ChangeId → spear `deliver` | `ticket` (status, pipeline, owner→user, priority, created_day) | `Count`, fk join to `user.team` (`EqU32Via`), `GroupReduce` | round-robin / queues (D-FLOW-5); ThreadId not in the event |
| **Mail** | spear placement; send via `EmailSubmission/set` | `message` (mailbox, ticket→ticket, seen, day) | inbox counts, thread rows (`Agg::Rows`) | thread_id column empty in spear; delegation absent in dir-sim |
| **Documents** | `Blob/upload`, `FileNode`, or tesseract-paperless intake | `document` (ticket→ticket, folder, size_kb); bytes in spear's content store | fk join to `ticket.status` | ACL model absent everywhere; SharePoint transport empty |
| **Delivery** | `delivery.*` + `queue.*` webhooks, `x:QueuedMessage` | `delivery` (message→message, outcome, day) | `GroupReduce` by outcome and by `message.mailbox` (`GroupAddr::Via`) | no MDN/DSN blobs, no open/click (D-MAIL-9 builds its own) |

## 5. P-QDB-1 — Quack as the database (measured)

See `.claude/board/LATEST_STATE.md` § P-QDB-1 for the table of numbers and the
command. At 3M tickets (30M rows across five tables), with fk-gathered
tables stored in fk order, every query is ≤ 25 ms except Q9 (58.6 ms, its
fk is random); with random fk order the delivery joins take 253 ms and
1.9 s. In short:
- HubSPO-rs implements Quack's `Registrar` + `Binder` in about 80 lines, with
  a codebook per field.
- Five tables are written to Lance, reloaded, and queried by nine Quack
  programs. Every answer equals a plain-loop oracle.
- Text literals bind once. Unknown literals and unknown fields are refused,
  never minted.

What Quack does not give, and what to do instead:
- **No strings in lanes.** Every text field is a codebook code. The codebook
  is the Binder's, so dir-sim `KeyId`s and status names live there.
- **No `ORDER BY`.** Inbox ordering by date stays a storage-order property:
  rows land in arrival order, so `Agg::Rows` already yields them in that
  order. Re-sort only at the edge.
- **No full-text search.** Subject and body search is the Tantivy index
  (tesseract-paperless precedent). It is not a Quack lane.
