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
- [ ] A recently verified, unexpired saved session starts from its private cache (up to one hour);
      expired/older sessions require online verification. Failed verification blocks gameplay.
- [ ] Completing Microsoft's browser flow with the approved Client ID returns to
      Horizon and verifies both Java entitlements and the matching official profile.
- [ ] A non-owning account cannot download/install game resources or enter worlds/servers.
- [ ] First-run setup downloads the game inputs and OpenJDK separately. Network interruption
      gives a retry, reuses completed verified files and does not enter an incomplete installation.
- [ ] Backgrounding/resuming setup preserves progress while the process remains alive;
      cancellation/expiry does not leave Java exporters running independently.
- [ ] Completed setup opens the title and retains worlds, options and touch layout.
- [ ] Real username, UUID and classic/slim skin are correct; an online-mode server accepts
      the session. Sign-in and joining servers have been observed by the tester; recheck on each release.
- [ ] Text fields open the keyboard, Back hides it and tapping the field reopens it.
- [ ] Multi-touch move/look/actions work together; touch layout changes persist.
- [ ] An active controller hides the touch HUD; touching the screen restores it. Unplugging
      a controller releases held buttons/axes.

The APK/version/signature checks, account regression tests, download/extraction tests and
isolated Java preparation checks are separate automated/development validation. They do not
prove real Microsoft sign-in or premium-server compatibility before those manual checks pass.

## 0.5.6 modal input, skins and reopening

- [ ] On Hypixel, tap the server-opened book's Done button and page/link buttons.
      Movement controls must disappear while the book is open and return after closing it.
- [ ] On CubeCraft, answer the required resource-pack prompt using touch. Proceed downloads
      and applies the pack; Disconnect leaves the server. Neither button moves the character.
- [ ] Test pause/options, signs, chat, inventories, creative tabs and server containers.
      Lists scroll with a swipe; dragging inventory items and sliders still works.
- [ ] In a populated lobby, inspect nearby players and the inventory avatar before and after
      applying/removing a resource pack. An unavailable/evicted custom skin falls back to a
      default skin rather than an invisible model. Intentional server invisibility is unchanged.
- [ ] Quit using Back/the menu, then reopen from the launcher three times without removing
      the task manually. Worlds, session, installed files and settings must remain intact.

The client already decodes Java resource packs directly. These fixes do not establish support
for every pack feature or replace testing the actual server pack. Do not report a pack as
successfully applied merely because its confirmation buttons respond.
