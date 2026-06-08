# Arx

A declarative system package manager for Arch Linux.

Arx manages your system packages through JSONC manifest files, similar in spirit to Nix or Ansible but built specifically around `pacman` and `libalpm`.

## Installation

```bash
cargo build --release
```

Requires `libalpm` (the pacman library) to be installed on your system.

## Usage

```bash
arx <COMMAND> [OPTIONS] <MANIFEST>
```

### Commands

- **`validate <manifest>`** -- Validate a manifest file for correctness.
- **`plan <manifest> [--json]`** -- Generate an install/prune plan. Shows what would be installed and removed.
- **`apply <manifest> [--dry-run]`** -- Apply a manifest: install missing packages and optionally remove undeclared ones.
- **`save-current <output>`** -- Snapshot the current system's installed packages into a JSONC manifest.

### Global Options

- `--root <path>` -- Override the root filesystem path (default: `/`).
- `--dbpath <path>` -- Override the pacman database path (default: `/var/lib/pacman`).

### Examples

Validate a manifest:

```bash
arx validate system.jsonc
```

Preview changes without applying:

```bash
arx plan system.jsonc
arx plan system.jsonc --json
```

Apply a manifest (dry run):

```bash
arx apply system.jsonc --dry-run
```

Apply a manifest (actually install/remove):

```bash
arx apply system.jsonc
```

Save current system state:

```bash
arx save-current system.jsonc
```

## Manifest Format

Arx uses a JSONC manifest format (JSON with comments and trailing commas).

### Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `include` | array of paths | no | Relative or absolute paths to other manifest files to include |
| `packages` | array of objects | yes | Package declarations |
| `options` | object | no | Top-level options |

### Package Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Package name |
| `source` | string | yes | One of `core`, `extra`, `aur`, `git`, or a repository name |
| `url` | string | only for `git` | Git repository URL |
| `depends` | array of strings | no | Runtime dependencies |
| `build_inputs` | array of strings | no | Build-time inputs |
| `build_options` | object | no | Build settings for the package |

### Build Options

| Field | Type | Description |
|-------|------|-------------|
| `ignore_default` | boolean | Skip top-level default build options for this package |
| `language` | string | Build language: `rust`, `c`, or `c++` |
| `toolchain` | string | Explicit toolchain command (e.g., `cargo`, `clang`) |
| `build_flags` | array of strings | Build flags for the package |

### Top-Level Options

| Field | Type | Description |
|-------|------|-------------|
| `remove_undeclared` | boolean | Enable prune semantics: remove installed packages not declared in the manifest |
| `default_build_options` | object | Default toolchain and flags for supported languages |

#### Default Build Options

| Field | Type | Description |
|-------|------|-------------|
| `toolchain_c` | string | Default C/C++ toolchain (e.g., `gcc`, `clang`) |
| `toolchain_rust` | string | Default Rust toolchain (e.g., `cargo`) |
| `build_flags_c` | array of strings | Default C/C++ build flags |
| `build_flags_rust` | array of strings | Default Rust build flags |

### Example Manifest

```jsonc
{
    "include": ["/other/files/to/include.jsonc"],
    "packages": [
        {
            "name": "firefox",
            "source": "extra"
        },
        {
            "name": "git",
            "source": "extra"
        },
        {
            "name": "kitty",
            "source": "extra"
        },
        {
            "name": "anyrun",
            "source": "git",
            "url": "https://github.com/anyrun/anyrun.git",
            "build_options": {
                "language": "rust",
                "toolchain": "cargo",
                "build_flags": []
            }
        },
        {
            "name": "hyprland",
            "source": "git",
            "url": "https://github.com/hyprwm/hyprland.git",
            "build_options": {
                "ignore_default": true,
                "language": "c++",
                "toolchain": "clang",
                "build_flags": [
                    "-DCMAKE_BUILD_TYPE=Release",
                    "-j2"
                ]
            }
        }
    ],
    "options": {
        "remove_undeclared": true,
        "default_build_options": {
            "toolchain_c": "clang",
            "toolchain_rust": "cargo",
            "build_flags_rust": ["--release"],
            "build_flags_c": [
                "-march=native",
                "-DCMAKE_BUILD_TYPE=Release",
                "-j4"
            ]
        }
    }
}
```

## How It Works

1. Arx reads your manifest and resolves all `include` directives recursively (with circular include detection).
2. It queries `libalpm` to determine which packages are already installed and resolves dependencies.
3. It computes a plan: packages to install (missing dependencies) and packages to remove (undeclared, if pruning is enabled).
4. On `apply`, it executes pacman transactions to install/remove packages.

## License

MIT
