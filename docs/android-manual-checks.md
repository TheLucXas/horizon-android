# Android release checks

These are pending manual checks, not a report that they passed. Use an update installation;
worlds and options stay in their existing app-data folder. Do not clear player data for testing.

- [ ] Before sign-in, the account screen is visible even without Minecraft resources.
      The mandatory screen has no Back button; its X and Android Back close the app.
      Reopening the app or using a controller does not enter a world. Cancel appears only
      while sign-in is in progress and returns to the sign-in screen.
- [ ] Existing resources from an older installation do not bypass account verification.
- [ ] A fresh installation detects Android language; a saved Minecraft language takes
      precedence. All sign-in text uses Spanish, English or Portuguese consistently;
      other languages fall back to English for this original bootstrap panel.
- [ ] First-run text, accented letters and button borders are crisp at native resolution.
- [ ] A saved session is revalidated online at startup. Failed verification blocks gameplay.
- [ ] After Mojang approves the Client ID, completing Microsoft's browser flow returns to
      Horizon and verifies both Java entitlements and the matching official profile.
- [ ] A non-owning account cannot download/install game resources or enter worlds/servers.
- [ ] First-run setup downloads the game inputs and OpenJDK separately. Network interruption
      gives a retry, reuses completed verified files and does not enter an incomplete installation.
- [ ] Backgrounding/resuming setup preserves progress while the process remains alive;
      cancellation/expiry does not leave Java exporters running independently.
- [ ] Completed setup opens the title and retains worlds, options and touch layout.
- [ ] Real username, UUID and classic/slim skin are correct; an online-mode server accepts
      the session. These checks remain blocked until application approval.
- [ ] Text fields open the keyboard, Back hides it and tapping the field reopens it.
- [ ] Multi-touch move/look/actions work together; touch layout changes persist.
- [ ] An active controller hides the touch HUD; touching the screen restores it. Unplugging
      a controller releases held buttons/axes.

The APK/version/signature checks, account regression tests, download/extraction tests and
isolated Java preparation checks are separate automated/development validation. They do not
prove real Microsoft sign-in or premium-server compatibility before those manual checks pass.
