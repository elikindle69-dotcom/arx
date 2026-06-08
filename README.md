# Arx - Declarative Package Manager for Arch Linux

Arx is a declarative package manager for Arch Linux that uses JSON manifests to define system packages, similar to NixOS but tailored for Arch/pacman.

## Features

- **Declarative manifests**: Define your system packages in JSONC format
- **Manifest includes**: Compose manifests by including other manifest files
- **Smart defaults**: Configure global build options that apply to all packages
- **Package sources**: Support for `core`, `extra`, `aur`, and `git` package sources
- **Dry-run mode**: Plan changes before applying them
- **Safe operations**: Requires explicit root permissions and user confirmation

## Installation

```bash
cargo install --path .
```

## Quick Start

### 1. Create a manifest

Create a file called `packages.jsonc`:

```jsonc
{
    "packages": [
        { "name": "firefox", "source": "extra" },
        { "name": "git", "source": "extra" },
        { "name": "kitty", "source": "extra" }
    ]
}
```

### 2. Validate the manifest

```bash
arx validate packages.jsonc
```

### 3. Plan changes

```bash
arx plan packages.jsonc
```

### 4. (Optional) Check removal impact

If your manifest has `remove_undeclared: true`, use check to analyze what will be removed:

```bash
arx check packages.jsonc
```

### 5. Apply changes

```bash
sudo arx apply packages.jsonc
```

Or skip the confirmation prompt:

```bash
sudo arx apply -y packages.jsonc
```

## Manifest Syntax

See [syntax.md](syntax.md) for complete documentation of the manifest format.

### Basic structure

```jsonc
{
    // Include other manifests (optional)
    "include": ["./base-packages.jsonc"],
    
    // Packages to install/manage
    "packages": [
        {
            "name": "package-name",
            "source": "extra"  // core, extra, aur, or git
        }
    ],
    
    // Global options (optional)
    "options": {
        "remove_undeclared": false,
        "default_build_options": {
            "toolchain_rust": "cargo",
            "build_flags_rust": ["--release"]
        }
    }
}
```

## Commands

### `validate <manifest>`

Validates that a manifest file is well-formed and adheres to the schema.

```bash
arx validate /path/to/manifest.jsonc
```

### `plan <manifest>`

Shows what packages would be installed, removed, or have build options applied without making any changes.

```bash
arx plan /path/to/manifest.jsonc
```

### `check <manifest>`

Performs comprehensive pre-flight analysis including package groups and dependency evaluation. Shows detailed information about packages that would be removed:
- Package group membership
- Size information
- **Dependency warnings**: Alerts if removing a package would break dependencies of declared packages

This is useful before applying a manifest with `remove_undeclared: true` to understand the impact.

```bash
arx check /path/to/manifest.jsonc
```

Example output with warnings:
```
⚠️  Removal Analysis (packages that will be removed):
  - grep 
    ⚠️  WARNING: Required by declared packages: base
  - nano
    ⚠️  WARNING: Required by declared packages: base
```

### `apply [OPTIONS] <manifest>`

Applies the manifest: installs missing packages and optionally removes undeclared packages.

**Options:**
- `-y, --yes`: Skip confirmation prompt

**Examples:**

```bash
# Apply with confirmation prompt
sudo arx apply packages.jsonc

# Apply without confirmation
sudo arx apply -y packages.jsonc
```

## Examples

See the [examples](./examples/) directory for complete example manifests.

## Limitations

- **AUR packages**: Not yet supported (warning shown, package skipped)
- **Git packages**: Not yet supported (warning shown, package skipped)
- **Build from source**: Build options are currently declarative only; actual building is not yet implemented
- **Dependency resolution**: Explicit package dependencies are supported but not automatically resolved

## Safety

- **Root required**: Package operations require root/sudo privileges
- **Dry-run by default**: Use `plan` to preview changes before applying them
- **Confirmation required**: `apply` requires explicit user confirmation unless `-y` flag is used
- **Pre-flight checks**: Packages are verified to exist before attempting installation

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

MIT License - see LICENSE file for details.
