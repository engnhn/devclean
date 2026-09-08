<p align="center">
  <img src="./assets/logo.svg" alt="devclean logo" width="180" />
</p>

<p align="center">
  <a href="https://github.com/engnhn/devclean/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/engnhn/devclean/ci.yml?branch=main&style=flat-square&label=ci" alt="ci status"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-mit-blue.svg?style=flat-square" alt="license"></a>
  <a href="https://github.com/engnhn/devclean/releases"><img src="https://img.shields.io/github/v/release/engnhn/devclean?style=flat-square&color=emerald" alt="release"></a>
</p>

# devclean

devclean scans project directories for common development artifacts and reports the disk space they occupy. it can also build a cleanup plan for selected artifact types and remove only the planned regenerable artifacts when `--execute` is used.

## quick start

one-line installation (linux):

```bash
curl -fsSL https://raw.githubusercontent.com/engnhn/devclean/main/install.sh | sh
```

or build from source:

```bash
cargo build --release
sudo cp target/release/devclean /usr/local/bin/
```

or install from a local checkout:

```bash
cargo install --path .
```

scan a projects directory:

```bash
devclean scan ~/projects
```

```text
Found 18 GB reclaimable

Artifact                   Size    Items
------------------ ------------ --------
node_modules             9.0 GB       14
target                   4.8 GB        6
.next                    3.4 GB        4
.gradle                  820 MB        2
```

show individual artifacts:

```bash
devclean scan ~/projects --details --limit 20
```

scan only projects un-modified for at least 30 days:

```bash
devclean scan ~/projects --older-than 30d
```

output machine-readable json:

```bash
devclean scan ~/projects --format json
```

## clean

`clean` is a dry run unless `--execute` is supplied. at least one `--type` is required.

```bash
devclean clean ~/projects --type node_modules --type target
```

```text
Cleanup plan

Size       Artifact           Recovery       Path
---------- ------------------ -------------- ------------------------------------------
619 MB     node_modules       npm install    ~/projects/web/node_modules
501 MB     target             cargo build    ~/projects/api/target

Would reclaim 1.1 GB
No files were removed.
```

clean only artifacts older than 14 days:

```bash
devclean clean ~/projects --type node_modules --older-than 14d --execute
```

## artifact types

| type | notes |
|---|---|
| `node_modules` | requires owning `package.json` |
| `target` | rust project with nearby `Cargo.toml` |
| `.gradle` | project-local gradle state |
| `.venv`, `venv` | python virtual environment structure |
| `__pycache__` | python bytecode cache |
| `.next` | next.js output with nearby `package.json` |
| `.nuxt` | nuxt output with nearby `package.json` |
| `dist` | node project output |
| `build` | node or gradle project output |

generic names such as `target`, `dist`, and `build` are only reported when nearby project files make them look like development artifacts.

## project protection with `.devcleanignore`

create a `.devcleanignore` file inside any project root to shield it from scanning and bulk cleanup operations:

```bash
touch ~/projects/heavy-cpp-project/.devcleanignore
```

## update & version

check installed version and target platform:

```bash
devclean version
```

self-update `devclean` to the latest release:

```bash
devclean update
```

## recovery and safety

recovery hints are shown in `--details` and cleanup plans. examples include `npm install`, `pnpm install`, `yarn install`, `cargo build`, `npm run build`, and `recreate env`.

recovery metadata is descriptive. it is not a delete recommendation.

cleanup rules:

- `scan` never deletes files
- `clean` requires explicit `--type` selection
- `clean` deletes only with `--execute`
- `.devcleanignore` protected projects are automatically skipped
- conditional and unknown recovery artifacts are blocked from execution
- cleanup paths must stay inside the scan root
- filesystem roots, the scan root, and the home directory itself are rejected
- symbolic links are not followed

## size accounting

on unix platforms, devclean uses allocated filesystem blocks from metadata and includes directory entries. this should broadly match tools such as `du`.

on non-unix platforms, devclean falls back to logical metadata length because rust does not expose allocated block counts consistently in the standard library.

hard-linked files are counted once per scan. `Modified` is filesystem modification age, not last use or last access.

## commands

| command | description |
|---|---|
| `devclean scan <path>` | scan without deleting anything |
| `devclean scan <path> --details` | show individual artifacts |
| `devclean scan <path> --older-than <days>` | scan artifacts older than specified threshold (e.g. 30d, 2w) |
| `devclean scan <path> --format json` | output machine-readable json report |
| `devclean clean <path> --type <type>` | print a cleanup plan |
| `devclean clean <path> --type <type> --older-than <days> --execute` | delete planned artifacts matching age threshold |
| `devclean version` | display version and build info |
| `devclean update` | self-update binary to the latest release |

## license

[mit](./LICENSE)
