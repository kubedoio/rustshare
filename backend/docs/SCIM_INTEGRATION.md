# SCIM Integration Guide

RustShare provides a SCIM-lite (webhook-style) API for enterprise identity provider (IdP) integration. This enables automated user provisioning, deprovisioning, and group management from your organization's IdP.

## Overview

This is a **SCIM-lite** implementation - webhook-style endpoints that support SCIM user/group lifecycle semantics without full RFC 7644 compliance. It satisfies contract I-05 for Phase 1 enterprise adoption.

### Supported Operations

| Operation | Endpoint | Description |
|-----------|----------|-------------|
| Provision User | `POST /api/v1/scim/users` | Create or update a user |
| Deprovision User | `DELETE /api/v1/scim/users/{external_id}` | Disable a user (soft delete) |
| Provision Group | `POST /api/v1/scim/groups` | Create or update a group |
| Delete Group | `DELETE /api/v1/scim/groups/{external_id}` | Delete a group |

## Authentication

All SCIM endpoints require bearer token authentication via the `Authorization` header:

```http
Authorization: Bearer <token>
```

The token is configured via the environment variable:

```bash
RUSTSHARE_SCIM_BEARER_TOKEN=your-secure-random-token
```

**Security Notes:**
- Use a cryptographically secure random token (at least 32 bytes)
- Store the token securely in your secrets manager
- Tokens are compared using constant-time comparison to prevent timing attacks
- SCIM endpoints return `503 Service Unavailable` if the token is not configured

## User Provisioning

### Create or Update User

**Endpoint:** `POST /api/v1/scim/users`

**Request Body:**

```json
{
  "external_id": "user@company.com",
  "user_name": "user@company.com",
  "name": {
    "given_name": "John",
    "family_name": "Doe"
  },
  "emails": [
    {
      "value": "user@company.com",
      "primary": true
    }
  ],
  "active": true,
  "tenant_id": "..."
}
```

**Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `external_id` | string | Yes | IdP's unique identifier for the user |
| `user_name` | string | Yes | Login identifier |
| `name.given_name` | string | No | First name |
| `name.family_name` | string | No | Last name |
| `emails` | array | No | Email addresses (first primary or first entry used) |
| `active` | boolean | Yes | Whether the user is enabled |
| `tenant_id` | UUID | No | Organization tenant (defaults to default tenant) |

**Responses:**

- `201 Created` - New user created
- `200 OK` - Existing user updated
- `400 Bad Request` - Invalid request body
- `401 Unauthorized` - Missing or invalid bearer token
- `503 Service Unavailable` - SCIM not configured

**Response Body (Success):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "external_id": "user@company.com",
  "action": "created"
}
```

### Deprovision (Disable) User

**Endpoint:** `DELETE /api/v1/scim/users/{external_id}`

The external_id should be URL-encoded if it contains special characters.

**Responses:**

- `204 No Content` - User disabled successfully
- `404 Not Found` - User not found
- `401 Unauthorized` - Missing or invalid bearer token

## Group Provisioning

### Create or Update Group

**Endpoint:** `POST /api/v1/scim/groups`

**Request Body:**

```json
{
  "external_id": "engineering",
  "display_name": "Engineering",
  "members": [
    { "value": "user1@company.com" },
    { "value": "user2@company.com" }
  ]
}
```

**Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `external_id` | string | Yes | IdP's unique identifier for the group |
| `display_name` | string | Yes | Human-readable group name |
| `members` | array | No | Array of member references by external_id |
| `members[].value` | string | Yes | External ID of the user to add |

**Responses:**

- `201 Created` - New group created
- `200 OK` - Existing group updated
- `400 Bad Request` - Invalid request body
- `401 Unauthorized` - Missing or invalid bearer token

**Notes:**
- Members are synced to match the provided list (members not listed are removed)
- Members must already exist as provisioned users
- Non-existent members are logged as warnings but don't fail the request

### Delete Group

**Endpoint:** `DELETE /api/v1/scim/groups/{external_id}`

**Responses:**

- `204 No Content` - Group deleted successfully
- `404 Not Found` - Group not found
- `401 Unauthorized` - Missing or invalid bearer token

## IdP Configuration Examples

### Okta

1. In Okta Admin Console, go to **Applications** → **Applications**
2. Find or create your RustShare application
3. Go to **Provisioning** tab → **Configure API Integration**
4. Check **Enable API Integration**
5. Configure:
   - **SCIM connector base URL**: `https://your-rustshare-instance.com/api/v1/scim`
   - **Unique identifier field for users**: `external_id`
   - **Unique identifier field for groups**: `external_id`
   - **Authentication mode**: HTTP Header
   - **Authorization**: Bearer token from `RUSTSHARE_SCIM_BEARER_TOKEN`

6. Enable desired provisioning actions:
   - Import New Users and Profile Updates
   - Push New Users
   - Push Profile Updates
   - Push Groups

### Azure Active Directory (Entra ID)

1. In Azure Portal, go to **Enterprise Applications** → **RustShare**
2. Go to **Provisioning** → **Get started** (or **Edit Provisioning**)
3. Set **Provisioning Mode** to **Automatic**
4. Configure **Admin Credentials**:
   - **Tenant URL**: `https://your-rustshare-instance.com/api/v1/scim`
   - **Secret Token**: Your `RUSTSHARE_SCIM_BEARER_TOKEN`
   - Click **Test Connection** to verify

5. Expand **Mappings** and configure:
   - **Provision Azure Active Directory Users**: Map to SCIM user fields
   - **Provision Azure Active Directory Groups**: Map to SCIM group fields

6. Configure attribute mappings:
   - `externalId` → `external_id`
   - `userName` → `user_name`
   - `name.givenName` → `name.given_name`
   - `name.familyName` → `name.family_name`
   - `emails[primary eq true].value` → `emails[0].value`
   - `active` → `active`

7. Set **Provisioning Status** to **On**

### OneLogin

1. In OneLogin Admin, go to **Applications** → **RustShare**
2. Go to **Configuration** tab
3. Enable **SCIM Provisioning**
4. Configure:
   - **SCIM Base URL**: `https://your-rustshare-instance.com/api/v1/scim`
   - **API Token**: Your `RUSTSHARE_SCIM_BEARER_TOKEN`
   - **API Token Type**: Header
   - **Header Name**: `Authorization`
   - **Header Value**: `Bearer <token>`

5. Go to **Provisioning** tab and enable desired actions

## Data Mapping

### User Mapping

| SCIM Field | RustShare Field | Notes |
|------------|-----------------|-------|
| `externalId` | `external_id` | IdP correlation key |
| `userName` | `username` | Login identifier |
| `name.givenName` | `name` | First name |
| `name.familyName` | `surname` | Last name |
| `displayName` | `display_name` | Constructed from name or userName |
| `emails[].value` | `email` | Primary email |
| `active` | `disabled_at` | `false` sets `disabled_at` timestamp |
| `groups` | `group_members` | via group provisioning |

### Group Mapping

| SCIM Field | RustShare Field | Notes |
|------------|-----------------|-------|
| `externalId` | `external_id` | IdP correlation key |
| `displayName` | `name` | Group name |
| `members[].value` | `group_members` | User external_ids |

## Idempotency

All SCIM operations are idempotent:

- **User Provisioning**: Provisioning the same user twice updates the existing user
- **Group Provisioning**: Provisioning the same group twice updates the existing group and syncs members
- **Deprovisioning**: Deprovisioning a non-existent user returns 404
- **Group Deletion**: Deleting a non-existent group returns 404

## Webhook URL Format

The base URL for all SCIM operations is:

```
https://<your-rustshare-instance>/api/v1/scim
```

Individual endpoints:

```
POST   /api/v1/scim/users
DELETE /api/v1/scim/users/{external_id}
POST   /api/v1/scim/groups
DELETE /api/v1/scim/groups/{external_id}
```

## Limitations

This SCIM-lite implementation has the following limitations compared to full RFC 7644:

1. **No GET endpoints** - This is a webhook-style push model only
2. **No PATCH support** - Use POST to update resources
3. **No filtering/querying** - Resources are referenced directly by external_id
4. **No bulk operations** - Process users/groups individually
5. **No schema discovery** - Schema is documented here
6. **Soft delete only** - Users are disabled, not deleted; groups are hard deleted

These limitations are acceptable for Phase 1 enterprise adoption and satisfy contract I-05.

## Troubleshooting

### Common Issues

**401 Unauthorized**
- Verify `RUSTSHARE_SCIM_BEARER_TOKEN` is set on the server
- Check the Authorization header format: `Bearer <token>`
- Ensure token matches exactly (no extra spaces)

**503 Service Unavailable**
- `RUSTSHARE_SCIM_BEARER_TOKEN` environment variable is not set
- Contact your RustShare administrator to configure SCIM

**400 Bad Request**
- Missing required fields (`external_id`, `user_name` for users; `external_id`, `display_name` for groups)
- Invalid JSON in request body

**Members not added to group**
- Users must be provisioned before being added to groups
- Check server logs for "Member not found" warnings

### Logging

SCIM operations are logged at the following levels:

- **INFO**: User/group created, updated, deleted
- **WARN**: Member not found, non-existent user/group deprovision attempts
- **ERROR**: Database errors, unexpected failures

Enable debug logging:

```bash
RUST_LOG=info,rustshare=debug
```

## Security Considerations

1. **Use HTTPS** - Never send SCIM requests over unencrypted HTTP
2. **Rotate tokens** - Periodically regenerate `RUSTSHARE_SCIM_BEARER_TOKEN`
3. **Limit token exposure** - Store token only in IdP and server environment
4. **Monitor logs** - Watch for unauthorized access attempts
5. **IP restrictions** - Consider restricting SCIM endpoint access to IdP IP ranges at the load balancer level
