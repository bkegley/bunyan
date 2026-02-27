# Version Locations

All version fields must stay in sync. The bump-version.sh script handles this automatically.

## Files

| File | Line | Format | Pattern |
|------|------|--------|---------|
| `package.json` | 4 | JSON | `"version": "X.Y.Z"` |
| `src-tauri/tauri.conf.json` | 4 | JSON | `"version": "X.Y.Z"` |
| `bunyan-core/Cargo.toml` | 3 | TOML | `version = "X.Y.Z"` |
| `bunyan-cli/Cargo.toml` | 3 | TOML | `version = "X.Y.Z"` |
| `src-tauri/Cargo.toml` | 3 | TOML | `version = "X.Y.Z"` |

## Notes

- Root `Cargo.toml` is a workspace manifest only — no version field.
- The `sed` patterns in bump-version.sh match on the current version string, so they only replace exact matches.
- If a new crate is added to the workspace, add its `Cargo.toml` to the script's cargo_file loop.
