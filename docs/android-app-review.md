# Android AppID review draft

Official form: https://aka.ms/mce-reviewappid

Do not submit claims about validation or publication until those steps are complete.
This is an independent Android port of https://github.com/ludovic111/horizon, not an
official Microsoft or Mojang product. Approval is not guaranteed by this implementation.

1. **EULA / Usage Guidelines:** read the linked documents; answer Yes only after understanding them.
2. **Contact information:** the owner's valid contact email associated with the Entra registration.
3. **Request type:** New AppID for Approval.
4. **Application name:** Horizon android personal.
5. **Application ID:** `3908277c-d619-4fc3-bc07-0c72b38ae12e`.
6. **Tenant ID:** copy the Directory (tenant) ID from the owner's Entra application overview; this identifier need not be published in the repository.
7. **Associated website:** https://github.com/TheLucXas/horizon-android (default Android branch).

8. **Justification draft (verify before submitting):**

> Horizon Android is an independent, open-source Android port of the Rust-based Horizon
> client with touch and gamepad controls. It uses Microsoft device authorization and
> Minecraft Services to verify Java Edition entitlements and the account profile before
> gameplay or first-run resource installation. It retrieves the authenticated profile
> and creates a valid session for online-mode servers. Game JARs, libraries and assets
> are downloaded on the user's device from official Mojang endpoints and checked against
> their published hashes. Required runtime data is generated locally; proprietary game
> files are not bundled in the source repository or APK. An open-source OpenJDK Android
> runtime is downloaded separately for the preparation tools, with its license retained.
> Microsoft credentials are entered only on Microsoft's website; session tokens remain
> in private Android application storage.

9. **Additional information draft:**

> Upstream project: https://github.com/ludovic111/horizon . The Android fork retains
> the upstream MIT license and third-party notices. Its original preparation tools run
> locally against the user's downloaded copy; no decompiled game code or prepared data
> is published. Original upstream documentation screenshots are retained as documentation,
> with Minecraft content remaining subject to its Usage Guidelines. The application's current Client ID receives HTTP 403 with an invalid
> app-registration response from Minecraft Services. We request review of this Client ID.

The complete exporter preparation has passed in an isolated Android target35 app.
The full authenticated first-run path still needs real account testing. Real sign-in and
premium-server testing are blocked by application approval and must not be reported
as passing beforehand. The form's screenshot says submissions are reviewed weekly;
this does not promise a particular response date.
