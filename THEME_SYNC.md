# Cross-Device Theme Synchronization - Implementation Complete

## Overview

The RustShare application now supports **cross-device theme synchronization**, allowing users to have a consistent light/dark mode preference across all devices and browsers where they log in.

## Features

### Theme Options
- **Light Mode** - Bright theme for daytime use
- **Dark Mode** - Dark theme for reduced eye strain
- **System Mode** - Automatically follows the operating system's theme preference

### User Interface
- **Theme Toggle Button** - Located in the header (top-right area)
  - Shows sun icon ☀️ when in light mode
  - Shows moon icon 🌙 when in dark mode
  - Click to instantly toggle between light and dark
  - Keyboard accessible

### Persistence
- **Local Storage** - Theme is saved in browser localStorage for instant loading
- **Backend Database** - Theme is synced to the user's account for cross-device access
- **Fallback Mechanism** - If backend is unavailable, uses localStorage

## Architecture

### Backend Components

#### Database
- **Migration**: `20260319000001_add_user_theme.sql`
- **Table**: `users`
- **Column**: `theme` (TEXT, NOT NULL, DEFAULT 'system')
- **Constraint**: CHECK (theme IN ('light', 'dark', 'system'))

#### Domain Model
- **Enum**: `Theme` (Light, Dark, System)
- **Location**: `backend/crates/core/src/domain/user.rs`
- **Methods**:
  - `User::set_theme(theme)` - Update user's theme preference
  - `Theme::to_string()` - Convert to database value
  - `Theme::from_str()` - Parse from database value

#### API Endpoints

**GET /api/users/me**
- Returns complete user profile including theme
- Used on login to load user's theme preference
- Response includes: id, username, email, display_name, is_admin, storage_quota, **theme**, created_at, updated_at

**PATCH /api/users/me/theme**
- Updates user's theme preference
- Request body: `{ "theme": "light" | "dark" | "system" }`
- Response: `{ "theme": "light" | "dark" | "system" }`
- Automatically updates `updated_at` timestamp

#### Repository
- **MetadataStore::update_user_theme(user_id, theme)** - Persists theme to database
- **MetadataStore::find_user_by_id(user_id)** - Returns user with theme
- **MetadataStore::find_user_by_email(email)** - Returns user with theme (for login)

### Frontend Components

#### Theme Store
- **Location**: `frontend/src/lib/stores/theme.ts`
- **State**: Writable store containing current theme
- **Methods**:
  - `setTheme(theme, syncToBackend)` - Set theme and optionally sync to backend
  - `toggleTheme(syncToBackend)` - Toggle between light/dark and sync
  - `loadFromBackend(theme)` - Load theme from API response
  - `getResolvedTheme()` - Get actual light/dark value (resolves 'system')

#### Auth Store Integration
- **Location**: `frontend/src/lib/stores/auth.ts`
- **On Login**: Automatically fetches user profile and loads theme
- **On Initialization**: Loads theme from backend if token is valid
- **Fallback**: Uses localStorage if API fails

#### API Client
- **Location**: `frontend/src/lib/api/users.ts`
- **Functions**:
  - `getUserProfile()` - Fetch user profile with theme
  - `updateUserTheme(theme)` - Send theme update to backend

#### UI Components
- **ThemeToggle Component**: `frontend/src/lib/components/common/ThemeToggle.svelte`
  - Displays current theme icon
  - Handles click to toggle theme
  - Shows tooltip with current theme
- **Header Integration**: Theme toggle added to header alongside help button

## User Flows

### First Login
1. User logs in with username/password
2. Frontend receives JWT token
3. Frontend calls `GET /api/users/me` to get user profile
4. User profile includes theme (default: 'system')
5. Theme is applied to the application
6. Theme is saved to localStorage as backup

### Changing Theme
1. User clicks theme toggle button in header
2. Theme store toggles theme (light ↔ dark)
3. New theme is immediately applied to UI (via `data-theme` attribute)
4. New theme is saved to localStorage
5. API call sent to `PATCH /api/users/me/theme` with new theme
6. Backend updates database and returns success
7. If API fails, theme still persists locally via localStorage

### Login on Another Device
1. User logs in on second device/browser
2. Same flow as "First Login"
3. Theme from backend is automatically applied
4. Both devices now have the same theme
5. Changing theme on either device updates both (after next login/refresh)

### System Theme Mode
1. User selects "system" mode (future enhancement)
2. Application respects OS theme preference
3. Theme automatically changes when OS theme changes
4. Still synced across devices

## Testing

### Manual Testing

**Test 1: Theme Toggle**
```
1. Open http://localhost in browser
2. Log in with admin credentials
3. Click theme toggle button in header
4. Verify UI switches between light/dark mode
5. Refresh page
6. Verify theme persists
```

**Test 2: Cross-Device Sync**
```
1. Log in on Chrome
2. Change theme to dark mode
3. Log in on Firefox (same account)
4. Verify dark mode is automatically applied
5. Change theme to light on Firefox
6. Log out and log in again on Chrome
7. Verify light mode is now applied on Chrome
```

**Test 3: Offline Fallback**
```
1. Log in and set theme to dark
2. Stop backend service (docker compose stop backend)
3. Refresh page
4. Verify dark theme still applies (from localStorage)
5. Try to toggle theme
6. Verify toggle works locally (fails to sync, but still functional)
7. Restart backend
8. Toggle theme again
9. Verify sync now works
```

### API Testing

**Get User Profile**
```bash
# Get JWT token from login
TOKEN="your_jwt_token_here"

# Get profile
curl -H "Authorization: Bearer $TOKEN" http://localhost/api/users/me

# Response:
{
  "id": "uuid",
  "username": "admin",
  "display_name": "Administrator",
  "email": "admin@localhost",
  "is_admin": true,
  "storage_quota": 10737418240,
  "theme": "system",
  "created_at": "2026-03-19T...",
  "updated_at": "2026-03-19T..."
}
```

**Update Theme**
```bash
# Update to dark mode
curl -X PATCH -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"theme":"dark"}' \
  http://localhost/api/users/me/theme

# Response:
{
  "theme": "dark"
}

# Verify in database
docker compose exec postgres psql -U rustshare -d rustshare \
  -c "SELECT username, theme FROM users;"
```

## Technical Details

### Database Schema
```sql
ALTER TABLE users ADD COLUMN theme TEXT NOT NULL DEFAULT 'system';
ALTER TABLE users ADD CONSTRAINT check_theme_value
    CHECK (theme IN ('light', 'dark', 'system'));
```

### Data Flow

```
User Action (Toggle Theme)
    ↓
ThemeToggle Component
    ↓
Theme Store (toggleTheme)
    ↓
├─→ Apply to DOM (data-theme attribute)
├─→ Save to localStorage
└─→ API Call (PATCH /api/users/me/theme)
    ↓
Backend Handler (update_user_theme)
    ↓
MetadataStore (update_user_theme)
    ↓
PostgreSQL Database (UPDATE users SET theme = ...)
    ↓
Response (200 OK)
```

### Error Handling

1. **API Failure**: Falls back to localStorage, user still gets local theme
2. **Invalid Theme**: Backend validates with CHECK constraint
3. **Unauthorized**: Theme update requires valid JWT token
4. **Network Error**: Theme still works locally, syncs on next successful API call

## Configuration

### Environment Variables
No additional environment variables needed - uses existing database and JWT configuration.

### Database Migration
Migration runs automatically on backend startup via sqlx migrations.

### Frontend Build
Theme feature is included in production build - no special configuration needed.

## Performance

### Impact
- **Database**: Single column added to users table
- **API Calls**: 1 additional call on login (GET /api/users/me)
- **API Calls**: 1 call per theme change (PATCH /api/users/me/theme)
- **Storage**: ~10 bytes per user in database
- **localStorage**: ~20 bytes per browser

### Optimization
- Theme is cached in memory (Svelte store)
- Theme change is instant (no loading state)
- API calls are async (non-blocking)
- Falls back to localStorage if API is slow/unavailable

## Future Enhancements

1. **System Theme Auto-Detection**
   - Currently user must manually select "system" mode
   - Could auto-detect and switch with OS theme changes
   - Already implemented in frontend, just needs UX for selection

2. **Theme Scheduling**
   - Automatically switch theme based on time of day
   - E.g., light during day, dark at night

3. **Custom Themes**
   - Allow users to create custom color schemes
   - Store color preferences in database

4. **Per-Page Themes**
   - Different themes for different sections of the app
   - E.g., dark for files page, light for settings

## Troubleshooting

### Theme Not Syncing Across Devices
- Check if user is logged in with same account
- Verify backend is running (docker compose ps)
- Check backend logs for errors (docker compose logs backend)
- Verify database migration ran successfully

### Theme Resets on Page Refresh
- Check browser console for JavaScript errors
- Verify localStorage is not disabled
- Check if authentication token is valid

### Database Error on Theme Update
- Verify migration ran: `docker compose exec postgres psql -U rustshare -d rustshare -c "\d users"`
- Check for theme column and constraint
- Restart backend if needed: `docker compose restart backend`

## Summary

✅ **Complete cross-device theme synchronization**
✅ **Three theme options**: light, dark, system
✅ **Instant UI updates** with smooth transitions
✅ **Backend persistence** in PostgreSQL database
✅ **Frontend fallback** to localStorage
✅ **Comprehensive error handling**
✅ **Zero configuration** required
✅ **Production ready**

The theme system is fully functional and ready for production use!
