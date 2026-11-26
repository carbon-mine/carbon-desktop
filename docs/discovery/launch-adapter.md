# Vanilla launch adapter comparison

This review covers planning for the pinned Minecraft 26.3 release and 24w14a snapshot on Windows and Linux x86-64. The local Rust planner is the selected development producer. Both interfaces have the same trusted Rust entry point available for future integration. Play remains gated.

## Corpus

| Input | Expected result |
| --- | --- |
| 26.3 release | Temurin 25.0.5.1+2, asset index 34, main class `net.minecraft.client.main.Main`. Modern argument arrays and native JARs on the classpath. |
| 24w14a snapshot | Temurin 21.0.13.1+2, asset index 16, the same main class, legacy `--userType` slot within the modern argument array and different native-library properties. |
| 1.8.9 release | Traditional `minecraftArguments` string, Java 8 and asset index 1.8. Comparison only. The Carbon planner refuses legacy arguments. |
| Forge 1.21.1-52.1.0 installer | Independently inspect `install_profile.json` inside the pinned archive. Three client-applicable processors reference InstallerTools, ForgeAutoRenamingTool and binarypatcher. Comparison only. |

The [vanilla pins](../../fixtures/launch-corpus/vanilla.json) identify both Mojang resources, SHA-1, SHA-256, asset-index digests and classpath counts. The [legacy and Forge pins](../../fixtures/launch-corpus/comparison.json) identify the other public inputs. Carbon's [runtime pins](../../crates/runtime/src/artifact.rs) bind the runtime archives by OS, version, size and SHA-256. Raw upstream inputs and binary artifacts live outside Git. The checked-in material contains identifiers, expectations and test data.

## Candidate matrix

| Candidate | Source and package evidence | Result |
| --- | --- | --- |
| In-house Rust resolver | The existing artifact plan resolves libraries in manifest order. The new argument pass consumes that order and known provider templates. The locked dependency graph adds two edges to existing crates. No added distribution package or sidecar. | Selected for the development scope. Both exact modern documents produced repeatable outputs. WSL2 traces witnessed zero helper attempts, network calls and file mutations during planning. |
| PortableMC 5.0.6 | Crates.io archive SHA-256 `6bc8077539b7945e7a711f0d909b14fb228a011ff94dedcec015b00bae1289c2`, 108,879 bytes compressed. Archive source `4a60d014f99d86be2a16e8ae397c5220d13176e6` is Apache-2.0. Its public `Game` struct packs supplied args into `Command`. The installer permits a fixed JVM path and download cancellation and still runs its own library, asset and JVM finalization. Forge directly calls `command.output()` on the processor. [Published crate](https://docs.rs/portablemc/5.0.6/portablemc/) · [Vanilla installer and command](https://github.com/theorzr/portablemc/blob/4a60d014f99d86be2a16e8ae397c5220d13176e6/portablemc/src/base/mod.rs) · [Forge processor](https://github.com/theorzr/portablemc/blob/4a60d014f99d86be2a16e8ae397c5220d13176e6/portablemc/src/forge/mod.rs) · [License](https://github.com/theorzr/portablemc/blob/v5.0.6/LICENSE) | Source comparison only. Building a `Game` directly still requires Carbon to resolve the command. The inspected installer-independent path did not produce a plan from Carbon's verified artifacts. Runtime measurements remain open. |
| XMCL `@xmcl/core` 2.16.3 and `@xmcl/installer` 6.3.7 | MIT, source `829c3f26c8ef635908921088812e1ef95ca76f86`. `generateArguments` uses an explicit resolved version and Java path. It can invent default player/token values and preserve unknown placeholders. `launch` separately runs filesystem prechecks and spawns Java. A custom `InstallRuntime` can override download, Java and materialization. The stock node runtime supplies its own filesystem operations and defaults to spawning Java in the absence of a hook. [Core source](https://github.com/Voxelum/x-minecraft-launcher/blob/829c3f26c8ef635908921088812e1ef95ca76f86/packages/core/launch.ts) · [Installer runtime](https://github.com/Voxelum/x-minecraft-launcher/blob/829c3f26c8ef635908921088812e1ef95ca76f86/packages/installer/installManifest.ts#L596-L624) · [License](https://github.com/Voxelum/x-minecraft-launcher/blob/829c3f26c8ef635908921088812e1ef95ca76f86/LICENSE) | Default npm install failed because `@xmcl/unzip@2.2.0` requests `@xmcl/yauzl` through `workspace:^*`. Its entry point also refers to an absent `index.ts`. A scratch override to that tarball's `dist/index.js` and a pinned yauzl package allowed the core-only probe. Counts matched across all four version/platform pairs. Two placeholders remained literal. The installer and helpers remain outside the dynamic result. |

The XMCL archive payloads can be verified against npm's `dist.integrity` values. Core 2.16.3 uses `sha512-6TXKVsqbi/FlBn6rNiubm1IkZhq8cDHnsvG7SVAW0PR+hAU/8ibj9hrZPOYyW3btgHJIUIln1rvo0ywAu5i5IQ==`. Installer 6.3.7 uses `sha512-onv8ig2C3HMJOf3Oqeilt/LWhFkUASQls23WNEJeGuhs1G2B6E4GqnKxbg8ilNt0rvZyxOxQh3PcebtCcNmJ+Q==`. Unzip 2.2.0 uses `sha512-W2Uw1OHtEqnCmxeZXgF+QO+2GKSy4upX+IXXUrNDVnW8a6SkiQV6z5q3fGIM5I9ry3TMP1STZ60mnovhz69uxg==`. The pinned alternatives contribute no redistributed code or notices to this patch. Revisit their notices and dependency graph if a later build embeds either library.

The compressed XMCL core, installer and unzip archives were 96,019, 339,967 and 8,314 bytes respectively. Those are source/package inputs. Installed size and resident memory for either upstream candidate remain unmeasured.

## Trusted API and restrictions

`carbon_vanilla::launch::plan_launch` takes trusted profile settings, a `PreparedInstanceView` and a `carbon_runtime::RuntimeLease` held for the same app root, a requested mode and an account reference. The activation digest includes the exact metadata and verified file map scoped to the instance and platform. The app-managed runtime is identified by its pinned archive digest. `LaunchPlan` borrows both leases and carries the trusted argument vector and account revision. `LaunchSummary` contains version, platform, mode, entry point, runtime release and argument counts.

The planner evaluates modern rules in manifest order with the last matching action. Its setup features enable only the profile dimensions. Carbon supplies bounded memory and omits the provider's optional default user JVM preferences. The only entry point is the vanilla main class, with explicit JVM flags, known dynamic properties and single argv elements for paths, including directories with spaces. Classpath order follows the verified install plan and appends the verified client JAR. The Java executable comes from the verified runtime. Legacy native extraction, inherited versions, processor commands, unknown selected placeholders, unknown features, unreviewed flags, unknown game switches and unsupported targets fail before a command can be assembled. Native search requires an immutable, host-verified generation. The JNA, LWJGL and Netty extraction properties remain unresolved until the host can qualify preparation and promotion of their native libraries. A writable game directory cannot supply native code. Identity placeholders remain unresolved. The planner never accepts tokens.

The next trusted runner must take exclusive ownership of the instance before admission, construct a fresh account reference from the CDESK-17 decision, and compare the current profile and activation under that ownership. `LaunchPlan::recheck` compares the account revision and selected generation, verifies unchanged profile launch settings and rehashes both leased inputs without repair or download. It assumes trusted callers and cooperative locks. Authenticity of the active receipt and the private filesystem still relies on the OS boundary. Use the activation digest as the installation binding in the account decision and recheck that decision at the final spawn gate. The protected state lease supplies the working directory and the typed `--gameDir` slot. Keep the state, view and runtime leases through the CDESK-22 drain. Native path mapping, environment scrubbing, JVM native extraction, launch admission, account-bound offline rules, the online session boundary and actual game execution require their own evidence.

## Repeat the comparison

Build the frontend first. Place the four public inputs under an ignored folder such as `.artifacts/launch-adapter`, using the URLs or existing verified bytes described in the pin files. Save the manifests under their version IDs with the `.json` extension and retain the Forge installer filename. The scripts read these files offline. Passing the corpus requires the pinned bytes and no commercial game JAR, signed-in profile, downloaded runtime or private world.

```powershell
bun run build
python scripts/verify-launch-corpus.py .artifacts/launch-adapter
$env:CARBON_LAUNCH_CORPUS_INPUT = (Resolve-Path .artifacts/launch-adapter).Path
cargo test --locked -p carbon-vanilla -p carbon-runtime
cargo test --locked -p carbon-vanilla pinned_official_corpus_is_offline_deterministic -- --ignored --nocapture
```

On Linux or WSL with `strace` and Python available, capture the process and file/network syscall trace. The script independently validates its detector with a benign helper, an unconnected socket and a temporary file write. The control runs separately from the planner.

```sh
bash scripts/trace-launch-corpus.sh .artifacts/launch-adapter .artifacts/launch-adapter/strace-in-house.log
```

The default Rust tests cover malformed or ineligible metadata, disallowed JVM flags, missing natives, same-length file tampering, unchanged saves, wrong Java, stale account revision or content activation, and argument and classpath stability. The official corpus exercises both platform rule sets against the expected classpath and asset-index selection. Both checks run against the shared Rust code. GPUI and Tauri wiring, native gameplay and Forge processor qualification remain open.

In one native Windows x86-64 debug run, the first in-memory argument pass took 0.43–0.62 ms across the four version/platform pairs. The mean of 50 warmed passes for each pair was 0.42–0.51 ms. The same four pairs under WSL2 and `strace` on the same host took 0.53–0.62 ms first pass and 0.47–0.58 ms warmed mean. Timing covers argument generation after loading the verified manifest bytes and building the ordered artifact map. The WSL2 test process peaked at 8,800 kB RSS. Its debug test executable was 12,289,640 bytes. The Windows debug test executable was 7,240,192 bytes. These measurements cover the test harness and shared crates. Marginal launcher binary size and full verification latency remain open.

The trace recorded the test executable's initial start and zero helper attempts, network calls or filesystem mutations. The control separately confirmed detection of all three effects. Windows and WSL2 passed 26 and 28 focused vanilla/runtime tests respectively, with six separately opted-in checks skipped in each default run. The pinned official corpus passed separately on both hosts.

The WSL2 corpus trace runs the in-memory argument builder with synthetic paths. Separate regressions check the actual view and runtime lease boundaries using benign files. The combined public `plan_launch` entry point with a real contained runtime and private state remains open.

### XMCL core probe

The optional [probe](../../scripts/probe-xmcl-launch.cjs) reads the same two public manifests and names the same logical synthetic layout. In an ignored scratch prefix, install `@xmcl/yauzl@2.10.0` with scripts disabled, unpack the verified core and unzip tarballs under that prefix's `node_modules/@xmcl`, and change only the staged unzip package's `main` to `./dist/index.js`. Run the probe under Node 24 with its permission model enabled, using the input directory, path to the staged core package and path to the vanilla pin file as its three arguments. The observed run had a private network namespace, read-only mounts for Node, its packages and both manifest inputs, and no Node write or child-process permission. The tracer ran outside Node. The explicit icon fixture suppressed icon discovery, and the synthetic paths provided no logging XML file.

Node 24.19.0 under WSL2 with `strace` produced the same classpath counts as the Rust run in all four cases. It kept `${clientid}` and `${auth_xuid}` literal, included `-DlibraryDirectory` and omitted the absent logging property. Platform overrides evaluated the target rules with the Linux host's classpath separator. The timed core-only call took 0.70–3.15 ms on first call and 0.66–1.05 ms warmed mean. The process peaked at 62,420 kB RSS. The trace recorded one initial Node start, zero helper attempts, zero network calls and zero file mutation calls. Timing starts after package import, metadata parsing and verification. This comparison used an explicit package-resolution workaround and a synthetic view. Native Windows Node output and full leased-input parity remain open.

### Redacted sample plan

```json
{
  "version": "26.3",
  "platform": "windows-x86_64",
  "mode": "offlineSinglePlayer",
  "mainClass": "net.minecraft.client.main.Main",
  "javaMajor": 25,
  "javaRelease": "25.0.5.1+2",
  "classpathEntries": 75,
  "jvmArguments": 15,
  "gameArguments": 24,
  "identitySlots": 5
}
```
