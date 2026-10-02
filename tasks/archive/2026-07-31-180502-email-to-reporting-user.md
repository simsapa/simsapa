# Email to the reporting user (phase 1)

**Task:** 8.7 of `tasks/2026-07-31-180502-tasks-picker-url-handling-and-chromebook-import-failure.md`
**Source:** PRD Appendix B — the four steps in §"The message" are **B.2 verbatim**
and their order is load-bearing (B.1): the File Selection Test writes the
interesting lines *into* `log.txt`, so a user who copies the log first sends an
empty result and the round trip is wasted.

**Distribution: Google Play closed testing** (decided 2026-08-07), *not* the
sideloaded beta APK. Three consequences that change the message:

- The artifact is the **AAB** from `make android-aab` — package
  `io.github.simsapa.app`, the same one they already have. Not
  `make android-beta-dist`, and the `io.github.simsapa.app.beta` package plays
  no part here.
- **It is not a second app.** A closed-testing track ships the *same* package,
  so joining the programme **updates their existing Simsapa** and leaving it
  puts them back on the production version. Any "installs alongside your normal
  Simsapa" wording would be wrong.
- No developer mode, no powerwash, no sideloading — which is what made this the
  right choice; see the note at the bottom.

Before uploading: `android/version.txt` is at **6** and the production install
observed on the test phone is versionCode **5**, so 6 is probably free — but
Play refuses a versionCode that is not strictly greater than *every* previous
upload, including ones never promoted to production. **Check the Play Console's
highest uploaded versionCode and bump `android/version.txt` if 6 is taken.**

---

## The message

> Subject: A test build to find out why the dictionary import fails
>
> Thank you again for the report about the dictionary `.zip` that would not
> import, and for the log file — it was genuinely useful. It showed me that the
> app was handed an empty file location by the system file chooser, which is not
> something I can reproduce here, since I have no Chromebook. So rather than
> guess, I have made a test build that measures what the file chooser actually
> hands over.
>
> The test version comes through the Play Store, so there is nothing unusual to
> install. If you follow the link below and join the testing programme, your
> Simsapa will update to the test version in the normal way (it can take a few
> minutes to appear). Your suttas, bookmarks and settings are untouched, and if
> you leave the programme later you simply go back to the normal version.
>
> **[testing programme link here]**
>
> The test version will not import anything — both buttons below only look and
> report.
>
> I have a test build with two new buttons that should tell me what is going
> wrong, without you needing to describe anything. Could you install it and do
> these four steps in order?
>
> 1. Open **Help → About** and press **File Selection Test**. When the file
>    chooser opens, pick **the same dictionary .zip file that would not import**.
>    The app will not import it — it only looks at what the file chooser handed
>    over.
> 2. Press **Run Storage Diagnostics** on the same screen, wait for it to
>    finish, then press **Copy** and paste the text into your reply.
> 3. Still in the About window, scroll down to the list of log files, press
>    **Copy Contents** on the most recent one, and paste that into your reply as
>    well. (If it is too long to paste, use **Save As…** and attach the file.)
> 4. If it is not too much trouble: repeat step 1 two or three more times,
>    choosing the same file from different places — your **Downloads** folder,
>    **Play files**, and **Google Drive** if that is where it came from. Then
>    copy the log again. Each attempt adds a few lines and it helps a lot to see
>    which locations behave differently.
>
> The order matters, I'm afraid: step 1 is what writes the useful lines into the
> log file that step 3 copies. If the log is copied first it will not have them.
>
> Three small questions, if you have a moment — one sentence each is plenty:
>
> - Which Chromebook is it, and which ChromeOS version (**Settings → About
>   ChromeOS**)?
> - When the file chooser opened in step 1, was it the ChromeOS **Files**
>   window, or a plainer file picker? And if you remember — was it the *same*
>   one you saw when the import failed?
>
> That last question is more useful than it looks: my best guess is that the app
> cannot understand the answer your Chromebook's file chooser gives, so knowing
> which chooser appeared points straight at it.
>
> No hurry, and thank you for the patience.

---

## Notes for us, not for the user

- **The last question is B.3's third item, and it is now the most valuable of
  the three.** Since D-3a the Android test launches the app's *own*
  `ACTION_OPEN_DOCUMENT` instead of going through Qt's `FileDialog`, so what
  appears is whatever ChromeOS resolves that intent to. If they report a
  **different** chooser than the one they saw when the import failed, that
  difference is itself a finding: it would mean Qt's dialog and a plain
  `ACTION_OPEN_DOCUMENT` reach different pickers, and the two blocks are not
  measuring the same thing.
- **Tone (B.4).** Ask for button presses and pastes, never for `adb` or logcat —
  closing that gap is exactly what D-14 was for. Do not ask them to read or
  interpret the log.
- **Reading what comes back (B.5).** Grep `log.txt` for `FILE-SELECTION-TEST:`;
  each run is one block with a counter and timestamp. **On an Android block read
  the raw-intent rows of §4A.5 first** — the raw URI and `QUrl(raw)` validity sit
  upstream of the scheme and encoding lines and can settle the question alone.
  Only if the raw URI parses cleanly do the original rows apply. If step 4
  produced several blocks, compare their scheme and encoded-form lines across
  locations before concluding anything from any single one. See
  [docs/file-selection-test.md](../docs/file-selection-test.md).

### Why Play closed testing, and what it costs us

**Sideloading was never really available here.** Installing an unknown-source
Android app in ARC normally requires the Chromebook to be in **developer
mode**, which involves a powerwash, and a managed or school-issued Chromebook
may forbid it outright. Asking that of someone doing us a favour was not
reasonable. Closed testing avoids all of it: a link, no developer mode, no wipe,
and the install stays signed by Play App Signing.

Two things it costs, both worth knowing before the upload:

- **We need their Google account address** to add them as a tester (or a
  tester-list link they can open while signed in). That is one more thing to ask
  for, so ask for it in the same reply as anything else outstanding.
- **The turnaround is slower than an email attachment.** A closed-track release
  goes through review, and the update then has to reach the device — so build in
  a day or two before expecting the report, and do not read silence as failure.

Two prerequisites on our side, neither of which the beta-APK route needed:

- `android/version.txt` must exceed **every** versionCode ever uploaded, not
  just the one in production (see the header note).
- The upload is `make android-aab`, which builds all three ABIs. **Do not build
  it from the Qt Creator interface** — its kits are single-ABI, and an
  arm64-only bundle is filtered off Intel/AMD Chromebooks, which is precisely
  this user's device. See
  [docs/android-multi-abi-and-chromeos.md](../docs/android-multi-abi-and-chromeos.md).
