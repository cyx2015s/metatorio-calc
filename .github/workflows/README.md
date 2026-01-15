# GitHub Workflows

This directory contains GitHub Actions workflows for the project.

## build-and-release.yml

Automatically builds binaries for multiple platforms and architectures.

### Triggers

- **Push to master**: Builds binaries and uploads them as artifacts
- **Release creation**: Builds binaries and attaches them to the release
- **Manual trigger**: Can be triggered manually via workflow_dispatch

### Supported Platforms

Currently configured targets:

- **Linux**:
  - x86_64 (Intel/AMD 64-bit)
  - aarch64 (ARM 64-bit)

- **macOS**:
  - x86_64 (Intel Macs)
  - aarch64 (Apple Silicon)

- **Windows**:
  - x86_64 (Intel/AMD 64-bit)

### Adding New Architectures

To add support for additional architectures, add a new entry to the `matrix.include` section in `.github/workflows/build-and-release.yml`:

```yaml
- target: <rust-target-triple>
  os: <github-runner-os>
  name: metatorio-<platform>-<arch>
```

#### Common Rust Target Triples

- Linux: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`
- macOS: `x86_64-apple-darwin`, `aarch64-apple-darwin`
- Windows: `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`
- FreeBSD: `x86_64-unknown-freebsd`

#### Example: Adding 32-bit Linux Support

```yaml
- target: i686-unknown-linux-gnu
  os: ubuntu-latest
  name: metatorio-linux-i686
```

### Cross-Compilation

The workflow uses Rust's built-in cross-compilation support. For Linux ARM targets, it installs the necessary cross-compilation toolchain automatically.

For more complex cross-compilation scenarios, consider using the [cross](https://github.com/cross-rs/cross) tool by modifying the build step:

```yaml
- name: Build binary
  run: cross build --release --target ${{ matrix.target }}
```

### Configuration Options

You can customize the workflow behavior by modifying:

- **Trigger branches**: Change the `branches` list under `on.push` (currently set to `master`)
- **Ignored paths**: Modify `paths-ignore` to skip builds for certain file changes
- **Build flags**: Add cargo features or flags to the `cargo build` command
- **Environment variables**: Add or modify variables in the `env` section

### Artifacts

Built binaries are uploaded as workflow artifacts and can be downloaded from the Actions tab for 90 days (default GitHub retention).

### Releases

When creating a GitHub release, the workflow automatically builds and attaches binaries for all configured platforms.
