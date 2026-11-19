# Deployment branding

The frontend accepts deployment-wide build-time values:

```sh
VITE_BRANDING_NAME="Example Org"
VITE_BRANDING_LOGO_URL="/branding/logo.svg"
VITE_BRANDING_THEME="ocean"
```

The product name is plain text, the logo must be a same-origin path, and theme choices are `rustshare`, `ocean`, or `forest`. Invalid values fall back to RustShare defaults. Place a configured logo in `frontend/static/branding/` before building.

These values are compiled into the frontend and are shared by every tenant in that deployment. Changing them requires a frontend rebuild and redeploy; there is no live admin editor or tenant-specific branding persistence in this slice.
