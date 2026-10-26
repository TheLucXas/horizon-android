# Credits and licenses

## Original Horizon

Horizon was created by **Ludovic / [ludovic111](https://github.com/ludovic111)**
and its [upstream contributors](https://github.com/ludovic111/horizon/graphs/contributors).
This is an independent Android fork of https://github.com/ludovic111/horizon,
based on commit `f2f58aca945b087e551514376511ef99ebb3a974`.
The original commit ancestry and copyright notice are retained.
The Horizon source is distributed under the original [MIT license](LICENSE),
Copyright (c) 2026 Ludovic. Existing authorship notices in individual files remain intact.

## Android adaptation

The Android port is maintained by [TheLucXas](https://github.com/TheLucXas).
Android implementation assistance: **Codex (gpt-6.1-sol)**, including source changes,
build tooling, touch/controller integration, Microsoft sign-in, verified-ownership
startup, official resource downloads, local preparation and bootstrap UI fixes.
Gemini provided technical discussion and planning assistance in the project chat.
These credits do not imply endorsement by the upstream maintainer or Mojang.

## Fonts, icons, windowing and graphics

- **Pixelify Sans**, The Pixelify Sans Project Authors: SIL Open Font License 1.1.
  [License](crates/horizon-theme/fonts/OFL-PixelifySans.txt),
  [source](https://github.com/eifetx/Pixelify-Sans).
- **Geist / Geist Mono**, The Geist Project Authors / Vercel: SIL Open Font License 1.1.
  [License](crates/horizon-theme/fonts/OFL-Geist.txt),
  [source](https://github.com/vercel/geist-font).
- **Pixelarticons**, Gerrit Halfmann: MIT.
  [License](crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt).
- **winit**, the winit contributors: Apache-2.0.
  [Retained source and license](third_party/winit); Android adaptation is recorded in this fork.
- **wgpu / gfx-rs**, the gfx-rs developers: MIT or Apache-2.0.
  The existing patched wgpu-hal source and both licenses remain in [third_party/wgpu-hal](third_party/wgpu-hal).
- **Android NDK libc++**, LLVM contributors: Apache-2.0 WITH LLVM-exception.
  [License and source reference](packaging/android/licenses).
- **Rust and Cargo**, Rust project contributors: [upstream copyright and licenses](https://github.com/rust-lang/rust/tree/master/LICENSES).
- **Cargo dependencies** retain their own licenses. See the
  [locked dependency inventory and available license/notice texts](third_party/dependency-licenses/README.md).
  That inventory includes build/workspace dependencies and does not imply every package is in the APK.

## Downloaded runtime and Minecraft resources

The separately downloaded Android OpenJDK runtime is built by
[AngelAuraMC](https://github.com/AngelAuraMC/angelauramc-openjdk-build), from OpenJDK
and its contributors. The runtime archive retains its own licenses and legal notices;
it is not bundled in this Git repository or in the APK. See
[the preparation design](docs/android-resource-preparation.md).

Minecraft is a trademark of Mojang/Microsoft. Mojang/Microsoft retain ownership of
their game files, textures, audio and other game content. Those game inputs are
obtained on the user's device from official download endpoints only after account
and Java Edition ownership verification. No game JAR, extracted texture pack,
sound pack or prepared game-resource tables are published here.

Upstream documentation screenshots, illustrations and branding are retained as
upstream documentation with their original attribution. Gameplay screenshots may
show Minecraft content; they do not grant a license to extract or redistribute
that content. See the [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines).

**NOT AN OFFICIAL MINECRAFT PRODUCT. NOT APPROVED BY OR ASSOCIATED WITH MOJANG OR MICROSOFT.**
The Android fork is maintained independently; issues concerning it can be reported
through its GitHub issue tracker, rather than attributed to the upstream author.
