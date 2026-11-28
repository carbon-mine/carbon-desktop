# Developer account simulation

This workflow runs the **real** GPUI Minecraft account dialog and the ordinary
`carbon_account::desktop::Accounts` job lifecycle against a deterministic,
in-process provider. It exists because the native account surface is complete,
but a runnable sign-in, switch, refresh, revocation and restart walkthrough
otherwise needs Carbon's approved Microsoft registration and an entitled
tester. CDESK-112 tracks the slice; CDESK-15 remains the production OAuth work.

The simulation is not a second account UI and does not fake panel state. The
same `AccountState` transitions, worker, popup, Home/Library readiness and
`prepare_launch` denial are used. Only the transport behind the existing device
flow and the credential backend are replaced.

## Fail-closed boundary

All four conditions are required:

1. the non-default `dev-account-simulation` Cargo feature;
2. a debug/test build (`cfg!(debug_assertions)`);
3. the explicit `--dev-account-simulation` startup flag;
4. an absolute `--fixture-root` that passes the existing dev-fixture ownership
   and ordinary-root checks.

There is no environment variable, stored setting or release/package switch. A
non-feature build rejects the flag. A feature-enabled release build rejects the
flag at the debug-build gate.

Synthetic mode never reads or writes:

- the ordinary `library-v1` or review library;
- the per-install `account-client.json` public client ID;
- a compiled `CARBON_MICROSOFT_CLIENT_ID`;
- the Windows Credential Manager / Linux Secret Service / macOS Keychain;
- Microsoft, Xbox or Minecraft Services;
- a real browser page or a real game process.

The account files live under
`<fixture-root>/dev-account-simulation/` behind
`dev-account-simulation.json`. `synthetic-credentials.json` is a plain test
fixture containing synthetic values; it is not a production vault and its
values authorize nothing.

No synthetic result can qualify gameplay. The signed-in synthetic profile still
receives `OfflinePolicyUnavailable` / `OnlineSessionUnavailable` from
`prepare_launch`, and `with_launch_authority` has no decision to admit.

## Build and run

```powershell
# Build the opt-in developer binary.
cargo build --locked -p carbon-desktop-gpui --features dev-account-simulation

# Seed one guarded fixture library, then open the real Home surface with the
# simulated account provider.
target\debug\carbon-desktop-gpui.exe `
  --seed-fixtures default `
  --fixture-root "$env:LOCALAPPDATA\CarbonDesktopOpenCode\dev-account-workflows\fixtures" `
  --dev-account-simulation

# Reopen the same root without reseeding. Saved synthetic accounts restore.
target\debug\carbon-desktop-gpui.exe `
  --dev-account-simulation `
  --fixture-root "$env:LOCALAPPDATA\CarbonDesktopOpenCode\dev-account-workflows\fixtures"
```

Open **Minecraft account** from the toolbar. The dialog is headed by a
**Test account simulation** panel. The ordinary Home/Library content is still
shown; fixture instances remain honestly **Setup required**.

## Walkthrough

1. **Sign in** — choose **Use Forest (test)** or **Use Coast (test)**, then
   **Start simulated sign-in**. The dialog shows a synthetic code and the
   simulated-browser message. No real browser opens.
2. **Pending** — leave it pending. No provider is contacted. **Cancel** (footer)
   removes the challenge and keeps the previous verified account.
3. **Approve** — **Approve simulated sign-in** completes the ordinary device
   poll/exchange path through Xbox, Minecraft, entitlement and profile replies.
   The account appears as **Active · Verified**.
4. **Two personas** — add the other persona with **Add test account** (the
   button label becomes **Start simulated sign-in**). Both rows are independent
   and switchable.
5. **Switch / refresh** — use the ordinary **Switch** and **Refresh account**
   buttons. A still-current verification switches without a provider call;
   refresh runs the simulated refresh exchange and rotates the synthetic token.
6. **Decline / expiry** — start another sign-in and choose
   **Decline simulated sign-in** or **Expire simulated sign-in**. The ordinary
   typed error and recovery copy appear.
7. **Entitlement denied** — select a persona, choose **Deny next sign-in**, then
   refresh that active account (the control refreshes it automatically when it
   is active). The state becomes an entitlement denial, and both play modes
   remain blocked.
8. **Revoked refresh** — choose **Revoke saved refresh** for the active persona
   and refresh. The simulated provider returns the ordinary `invalid_grant`
   path: the saved credential is removed and sign-in is required again.
9. **Sign out** — the ordinary confirmation removes only that persona and keeps
   instances/saves untouched.
10. **Restart / restore** — close and reopen the same `--fixture-root`. The
    saved personas restore through the simulated refresh path. Scenario controls
    reset to granted on a new process; saved accounts and the selected persona
    persist in the isolated test root.

Synthetic codes are labelled **Synthetic sign-in code** and never ask for a
Microsoft password, token or real device code. The visible account names are
the fixed test identities below; they are not real Minecraft profiles.

## Fixture identities

| Persona | Profile UUID | Display name | Default synthetic entitlement |
| --- | --- | --- | --- |
| Forest | `11111111-1111-4111-8111-111111111111` | `Forest` | granted |
| Coast | `22222222-2222-4222-8222-222222222222` | `Coast` | granted |

Synthetic public client: `5eed1120-0000-4000-8000-000000000001`. It is not a
Microsoft application registration.

## Tests

```powershell
# Simulation protocol, typed outcomes, two personas, denial and revocation.
cargo test --locked -p carbon-account --features dev-simulation

# Feature/debug/root gates and marker ownership.
cargo test --locked -p carbon-desktop-gpui --features dev-account-simulation

# The ordinary production suites still pass without the feature.
cargo test --locked -p carbon-account
cargo test --locked -p carbon-desktop-gpui
```

Focused coverage:

- `simulation::tests` exercises pending, approve, decline, expiry, two personas,
  entitlement denial, refresh revocation and the absence of a network client.
- `desktop::tests::simulated_accounts_sign_in_switch_restore_deny_and_remove`
  drives the real `Accounts` worker through sign-in, switch, restart, denial,
  sign-out and denied `prepare_launch`.
- `dev_account_simulation::tests` proves the feature/debug/flag/root gates,
  marker provenance, refusal of an unsupported marker and refusal of the
  ordinary per-user root.

## Real developer end-to-end testing

The simulation does not replace a real provider check. For legitimate
end-to-end testing, use the same production pathway with isolated developer
data and an **approved Carbon deployment client ID plus an entitled tester
account**:

1. Build normally without `dev-account-simulation`, supplying the approved
   public client through the stock `CARBON_MICROSOFT_CLIENT_ID` compile-time
   configuration or the existing per-install approved-client-ID flow. Do not
   commit the ID or any credential.
2. Use a disposable or dedicated developer root, never the owner's ordinary
   library or vault, and keep the tester account's consent explicit.
3. Sign in through the real Microsoft device page, complete the Xbox and
   Minecraft exchanges, and verify the profile and Java entitlement.
4. Exercise switch, refresh rotation, revocation, sign-out and restart against
   that real account. Record the client configuration source, tester
   entitlement and app-root isolation, but not tokens, codes or personal data.
5. Treat play authorization separately. A successful account check still does
   not by itself authorize protected gameplay; follow the launch/provider
   prerequisites in [Play authorization](play-authorization.md).

This change does not perform a real sign-in, provider call, game launch or
production-vault mutation.

## Native verification, 29 November 2026

The Windows x64 debug executable built with `dev-account-simulation` was driven
through UI Automation from the real Home toolbar against a seeded guarded root.
The pass covered: opening the real account window; the test-account provenance;
pending/approve; Forest sign-in; adding Coast; switching back to Forest;
refresh; entitlement denial; decline; expiry; saved-refresh revocation and
recovery after restoring the grant; sign-out confirmation; and a separate
process restart that restored the saved Coast account through the simulated
refresh path. Every dialog kept the synthetic-browser label and the
"protected game launch is unavailable" readiness copy. No real browser,
credential prompt, provider request or OS vault mutation occurred. Evidence
trees and scripts stayed outside Git in the worker's scratch directory.

A launch-path smoke was not run: the seeded instances remain **Setup required**
and no safe existing no-entitlement fixture process was available that could be
started without a real runtime or game content. The account tests assert that
both play modes still deny admission after synthetic sign-in.

## Honest gaps

- The simulated provider is a deterministic fixture, not evidence of Microsoft,
  Xbox or Minecraft policy, latency, throttling or entitlement behavior.
- It covers the account lifecycle and UI states named above. It is not a
  replacement for the scripted transport regressions in `carbon-account` or the
  real approval in CDESK-14.
- Simulation scenario controls reset when the process restarts; saved test
  accounts and selection persist.
- A launch-path smoke test is separate. Only the existing benign
  no-entitlement fixture process may be used, and it must be reported as such.
