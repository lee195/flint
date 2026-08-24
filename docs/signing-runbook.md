# Flint signing + notarization runbook (3a-M1)

> The exact path from unsigned to Gatekeeper-clean `.dmg`. **Status 2026-08-24: BLOCKED — 0
> code-signing identities on this machine** (`security find-identity -v -p codesigning`). The
> spike's commands are pinned here so the moment a Developer ID Application cert lands, the
> signed+notarized path runs end-to-end. Current builds are **ad-hoc (linker-signed)** —
> `spctl --assess` rejects them (expected; fine for author-only validation, doc 05).

## When is signing required?

Author-only validation runs may be unsigned (doc 05). **Signing is a day-1 rule the moment a
build leaves this machine.** Both apply the moment we have a cert: sign the app **and** the
nested sidecar binaries (opencode now; `llama-server` in 3b) — the nested-sidecar
notarization is the highest-risk unknown (doc 05), so it's the first thing this spike proves.

## Prerequisites (once a cert exists)

1. **Developer ID Application certificate** in the login keychain:
   - Personal account: Xcode → Settings → Accounts (or developer.apple.com → Certificates)
     → create "Developer ID Application" → download → double-click to install.
   - Record the **team ID** (last 10 chars of the identity string, e.g.
     `Developer ID Application: Jisu Lee (XXXXXXXXXX)` → `XXXXXXXXXX`).
2. App-specific password or API key for `notarytool` (Apple ID account security page) — used
   only for notarization, never stored in the repo (env vars or Keychain).
3. No cert → **stop here**; this runbook stays the reference and the unsigned `.dmg`
   (`deno task tauri build`) is what we ship internally.

## Configure Tauri (tauri.conf.json)

```jsonc
"bundle": {
  "active": true,
  "targets": ["dmg", "app"],
  "macOS": {
    "signingIdentity": "Developer ID Application: Jisu Lee (XXXXXXXXXX)",
    "entitlements": "entitlements.plist",
    "hardenedRuntime": true
  }
}
```

`entitlements.plist` (in `src-tauri/`) — hardened runtime defaults; add sidecar-specific
entitlements as the 3a-M1 nested-sidecar spike determines (none expected for opencode, a
plain Mach-O; llama-server/Metal may need `com.apple.security.cs.allow-jit` — decide in 3b):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-jit</key><true/>
</dict>
</plist>
```

## Sign the nested sidecars before the app

The app's `codesign` won't reach inside `Contents/MacOS` sidecars for hardening. Sign each
sidecar first (this is where the `deno task bundle-opencode` recipe lands in 3a-M2):

```bash
codesign --force --options runtime --sign "Developer ID Application: Jisu Lee (XXXXXXXXXX)" \
  src-tauri/binaries/opencode-aarch64-apple-darwin
```

## Build signed + notarized (two paths)

**Path A — manual `notarytool` (primary; version-independent):**
```bash
deno task tauri build
APP=target/release/bundle/macos/flint.app
# sign the .app if Tauri's signingIdentity didn't (should be done by tauri build)
codesign --verify --deep --strict --verbose=2 "$APP"
# notarize the app (zip it first — notarytool takes a zip or a dmg)
ditto -c -k --keepParent "$APP" /tmp/flint-notarize.zip
xcrun notarytool submit /tmp/flint-notarize.zip \
  --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" --team-id XXXXXXXXXX \
  --wait --output-format json
xcrun stapler staple "$APP"                       # attach the notarization ticket
spctl --assess --type execute --verbose=4 "$APP"  # must print "accepted"
# re-zipped + notarized via the .dmg:
xcrun notarytool submit target/release/bundle/dmg/flint_0.1.0_aarch64.dmg \
  --apple-id "$APPLE_ID" --password "$APPLE_APP_PASSWORD" --team-id XXXXXXXXXX --wait
xcrun stapler staple target/release/bundle/dmg/flint_0.1.0_aarch64.dmg
```

**Path B — Tauri-native** (needs a tauri-cli that supports notarization; same env creds):
```bash
APPLE_ID=... APPLE_PASSWORD=... APPLE_TEAM_ID=XXXXXXXXXX deno task tauri build
```

## Verify (the "cert arrives" acceptance checklist)

```bash
security find-identity -v -p codesigning          # the Developer ID identity is present
codesign --verify --deep --strict --verbose=2 "$APP"
codesign -dv --verbose=4 "$APP"                   # Signature=Developer ID Application, TeamIdentifier set
spctl --assess --type execute --verbose=4 "$APP"  # "accepted, source=Notarized Developer ID"
spctl --assess --type execute --verbose=4 /Volumes/flint/flint.app   # same, from the mounted dmg
xcrun stapler validate "$APP"                     # ticket attached
# Fresh-machine double-click test: download the .dmg on a clean path, open it —
# Gatekeeper must not block; app must run chat + agent with the bundled opencode.
```

## Rollback / notes

- Ad-hoc state is fine for this machine's dev loop — do **not** commit the signingIdentity
  or credentials anywhere; the runbook references env vars only.
- The `.dmg` itself needs no separate signing (it wraps the signed `.app`); notarizing the
  dmg covers the bundle.
- Re-sign order matters: sidecars → app → dmg/notarize → staple. A modified binary inside an
  already-signed app invalidates the seal (this is the failure to watch for when 3b adds
  `llama-server`).
