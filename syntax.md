# Basic syntax

Arx uses a JSONC manifest format that supports comments and trailing commas.

Supported top-level fields:

- `include`: optional array of relative or absolute paths to other manifest files
- `packages`: required array of package declarations
- `options`: optional top-level options

Supported package fields:

- `name`: package name
- `source`: one of `core`, `extra`, `aur`, `git`
- `url`: required for `git` packages
- `depends`: optional array of runtime dependencies
- `build_inputs`: optional array of build-time inputs
- `build_options`: optional object with build settings

Supported build option fields:

- `ignore_default`: skip top-level default build options for this package
- `language`: e.g. `rust`, `c`, or `c++`
- `toolchain`: explicit toolchain command
- `build_flags`: build flags for the package

Top-level `options`:

- `remove_undeclared`: enable prune semantics for packages not declared in the manifest
- `default_build_options`: default toolchain and flags for supported languages

Example manifest:

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
