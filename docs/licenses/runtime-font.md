# Packaged runtime font contract

`litd-game` embeds the unmodified `Ubuntu-Light.ttf` bytes exported by
`epaint_default_fonts` version `0.33.3`. The font is infrastructure of the
packaged player host, not authored map content, so `.litdgame` packages must not
duplicate it.

The source-byte contract is:

- source: `epaint_default_fonts@0.33.3/fonts/Ubuntu-Light.ttf`
- bytes: `361676`
- SHA-256: `80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70`
- license: Ubuntu Font Licence 1.0
- notice: `docs/licenses/Ubuntu-Font-License-1.0.txt`

The runtime verifies the expected source hash before font parsing and exposes
the source version, license, notice hash, source hash, atlas hash, and text-plan
hash through its persisted runtime state. It has no system-font or environment
fallback.

The font and notice are compiled into `litd-game`. Any installer or portable
distribution that exposes notices as files must also install the notice above
as `licenses/Ubuntu-Font-License-1.0.txt`; it must not substitute a different
font under the same source id/hash.
