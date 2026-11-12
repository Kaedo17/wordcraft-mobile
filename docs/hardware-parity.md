# Hardware parity with Microsoft Word

> **Last reviewed:** 2026-10-12 · **Last updated:** 2026-11-12 · **Change:** minor (Android: the arm64 port re-based onto upstream v0.18.1, so the device-verified touch and mouse paths ride on the current engine; previously trivial: newly opened window dimensions are constrained to fit available screen dimensions, #5871); previously trivial (print helpers erase loaded plaintext inputs before driver work, #5710); previously trivial (Windows print submissions use isolated helpers so a cancelled driver cannot block a retry, #5710); previously trivial (print cancellation remains available without a document; sending-boundary limitation clarified, #5710); previously trivial (a print job waiting on its driver can be cancelled, and quitting doesn't wait for it, #5710); previously trivial (the printer list no longer waits on printer drivers, #5710); previously trivial (Linux Read Aloud: a sentence a screen reader cuts off is said again); previously trivial (ink page lookup work, #3614); previously trivial (only the PDF sent to the printer has no link annotations; Save these pages as PDF keeps them, #4734); previously trivial (a document with links prints: the print job's PDF has no link annotations, #4734; previously trivial: Pen / stylus row: two-stroke and closed-head arrows; previously trivial: Pen / stylus row: ink, eraser and Ink to Shape incl. arrows landed; previously trivial: a window too big for its screen shrinks to fit when it opens, #1215; previously trivial: 32-bit Windows starts on OpenGL; previously major: first version) · **Target:** Microsoft Word (Microsoft 365) for Mac 16.113.4 and Word for Windows

> **Printing update:** 2026-10-12 · direct desktop printing through craft-print; details below.

Word is not a hardware-heavy app, but a few devices matter to its users: printers above all, then
pens, microphones (Dictate) and speakers (Read Aloud), and displays.

**Dimension: ~40% (estimated), 20–40 h to full** (pen/ink hours are counted with the Draw tab in
[`target-app-parity.md`](target-app-parity.md)).

| Hardware | Word | WordCraft macOS | Windows | Linux / BSD | Web | Parity | Hours |
|---|---|---|---|---|---|---|---|
| Printers: native print dialog, printer choice, copies, duplex, page ranges, tray, scaling | ✅ (`WordPDE.plugin` print-dialog extension on Mac) | 🟡 Direct CUPS printing; printer, copies, collation, duplex, ranges, grayscale, scale to paper size, N-up and booklets; on every desktop the PDF sent to the printer leaves out link annotations, which the spooler can't fit to the paper; Save these pages as PDF keeps them (#4734) | 🟡 Direct spooler printing; printer, copies, collation, duplex, ranges, grayscale, scale to paper size, N-up and booklets; on every desktop the PDF sent to the printer leaves out link annotations, which the spooler can't fit to the paper; Save these pages as PDF keeps them (#4734) | 🟡 Direct spooler printing; printer, copies, collation, duplex, ranges, grayscale, scale to paper size, N-up and booklets; on every desktop the PDF sent to the printer leaves out link annotations, which the spooler can't fit to the paper; Save these pages as PDF keeps them (#4734) | ✅ browser print dialog (#209) | 35% | 5–8 |
| GPU rendering, HiDPI/Retina, pixel-aligned text | ✅ | ✅ wgpu + vello_cpu rasteriser, whole-pixel text (#110, #165) | ✅ DirectX 12 default (#50), OpenGL when DirectX 12 has no GPU driver (#316) or on 32-bit; startup crash on Intel UHD (#170) | ✅ (soft text on KDE at some zooms, #140) | ✅ | 80% | 3–5 (bugs) |
| Pen / stylus: ink, pressure, eraser, ink to shape/math | ✅ (Draw tab, `InkRender.bundle`) | 🟡 ink, stroke eraser, Ink to Shape incl. one- and two-stroke arrows with open or closed heads (#307, #395); pressure and Ink to Math missing | 🟡 same | 🟡 same | 🟡 same | 50% | (Draw tab, 15–25) |
| Multiple monitors, per-monitor DPI, remembered window position, a window that fits its screen | ✅ | ✅ (#38, #76, #1215) | ✅ (winit DPI fix pending, #155) | ✅ | — | 85% | 1 |
| Microphone: Dictate | ✅ (cloud speech service) | ❌ | ❌ | ❌ | ❌ | 0% | 8–15 + owner (speech model choice) |
| Speakers: Read Aloud | ✅ | ✅ system speech, speed, sentence skip (#190) | ✅ | 🟡 espeak-ng, then espeak, then Speech Dispatcher (over its socket), each sentence in its language's voice; packages recommend a speech program; silent in the Flatpak (no audio access, no speech program) | 🟡 | 75% | 1–2 |
| Touch screens and touchpads: pinch zoom, inertial scroll | ✅ | ✅ pinch (#179) | ✅ | ✅ inertial scroll (#252) | ✅ | 85% | 0–1 |
| Scanners/cameras (Insert from device / Continuity Camera on Mac) | ✅ | ❌ | ❌ | ❌ | ❌ | 0% | 2–4 |

**On Android** (the arm64 build, verified on a Samsung Galaxy Tab S11, SM-X736B, Android 16): the
same interface runs on the touch screen — a finger pans the page and scrolls the browser's storage
list, a still hold selects the word under the finger (with its handles) and opens the context menu
over it, a tap places the caret and a double-tap selects a word; a touch-made selection grows by
dragging its two teardrop handles, character by character, the way Word on touch does — the
drag-growth paths still stand down for touch, so a finger never drag-selects. Keyboard modifiers are
stamped onto each frame's key events, so chords reach egui (the on-device pass exercised typing;
no Bluetooth keyboard was attached), and the vendored winit fills Android's mouse path, so a mouse
attached to the tablet keeps hover, wheel and right button, verified on device with the signed
release build: a single click places the caret only (on a word, or invisibly on a punctuation
character), a double-click selects a word, a triple-click a paragraph, a press-drag selects, the
right button opens the context menu, and the wheel and hover work; its primary button is deduped,
because devices report a click through both
Android's Down/Up and ButtonPress/Release pairs, and one physical click must be one click — and the
button that changed comes from a diff of the button state, not `AMotionEvent_getActionButton` (an
API 33 symbol some devices' `libandroid` omits, which failed the whole library to load on an API 31
Huawei MediaPad, AGS5-W09). A tap so fast its down and up reach the app in one pass still counts as
touch, and a handle dragged onto the selection's other end keeps the drag alive (both handles stack
at the caret, as Word's do) so pulling past regrows the selection the other way. Options ›
General offers a 50% interface size for the tablet's 2560×1600 landscape screen.

The port re-based onto upstream v0.18.1 in 2026-11 carries the same input stack; its device
re-check on the two tablets is pending.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-12 | minor | Android: the port re-based onto upstream v0.18.1; the Tab S11 and MediaPad device re-check of the new build is pending |
| 2026-11-07 | trivial | Newly opened secondary windows constrain their initial viewport dimensions to the monitor's usable area, preventing windows from exceeding screen width on scaled displays (#5871) |
| 2026-11-06 | trivial | Print helpers remove loaded PDF/settings files before driver work, tolerate prior removal and report cleanup errors; a held-driver regression checks the files are already absent (#5710) |
| 2026-11-06 | trivial | Windows print submissions use a separate helper per job, preserving provider serialization inside the helper. An acknowledged cancellation stops that helper, so a stalled cancelled driver cannot block a later job; normal shutdown reaps helpers and removes temporary PDFs. The helper erases its plaintext inputs before driver work; a hard app crash during startup can still leave private temporary files. Startup requires an explicit start grant; after sending begins, jobs complete (#5710) |
| 2026-11-06 | trivial | File › Print: before sending starts, Cancel printing (`ui.printCancel`) stops a waiting job and frees the Print page while keeping its settings, including after the last document closes; a successfully cancelled job is never sent. Quitting cancels waiting jobs and joins jobs already sending (#5710). On Windows, winprint repeats the DEVMODE conversion after sending starts. A stall during that repeat prevents cancellation and can keep shutdown waiting, even before a native job exists. |
| 2026-11-06 | trivial | On Windows the Print panel lists printers without asking their drivers anything, removing the driver query a networked printer in power save could hold, which kept the list empty with Refresh unavailable (#5710; likely also behind the long "Finding printers" waits of #5645 and #5413): discovery uses craft-print's `printer_queues` (storytold/craft-libs#618), and printing still checks the job's options with the driver; physical sleeping-printer output remains to be confirmed by the reporter |
| 2026-10-28 | trivial | Ink page lookup work, #3614. Drawing order, geometry and saved data retain their behavior; no readiness estimate change |
| 2026-11-01 | trivial | N-up and booklet print imposition (#641): integrate current print-job link handling (#4734), preserve markup preview and sheet/copy planning; readiness estimates unchanged. |
| 2026-10-30 | trivial | Only the PDF sent to the printer leaves out link annotations; Save these pages as PDF (`file.printPdf`) keeps them (#4734) |
| 2026-10-29 | trivial | A document with links prints: the print job's PDF leaves out link annotations, which the spooler refused to fit to the paper (#4734) |
| 2026-10-28 | trivial | A window restored to a place no monitor covers opens centred on the primary monitor. The check compared monitor sizes only, so when a monitor was replaced by one of the same size (a dock unplugged, a display rearranged), the window opened off screen and the app looked like it quit at once |
| 2026-10-26 | trivial | Linux Read Aloud: a sentence another program's message cancels (a screen reader's announcement) is said again after a short pause, twice at most; cancelled again, paused by another program, or never begun, it pauses reading with a message |
| 2026-10-21 | trivial | Linux Read Aloud review fixes: Speech Dispatcher is found at `SPEECHD_ADDRESS` (`unix_socket`, `inet_socket`) or the default socket, gets the sentence's language and region, and a sentence another program cancels or pauses (a screen reader's message, a stop for every program) pauses reading on it with a message instead of being skipped or said over the other program |
| 2026-10-18 | trivial | Linux Read Aloud: Speech Dispatcher (over its socket, with the sentence's language; stopping cancels only WordCraft's own speech) when neither espeak-ng nor espeak is installed, a message naming the program to install when none is, and a speech program recommended by the `.deb` (`espeak-ng` or `speech-dispatcher`) and `.rpm` (`espeak-ng`) packages |
| 2026-10-15 | trivial | Pen / stylus row: Ink to Shape also makes two-stroke arrows and arrows with a closed triangle head |
| 2026-10-17 | minor | File › Print's Scale to paper size (#2108): pick Letter, Legal, A4 or any Layout › Size paper and every page is scaled to it and centred, the paper turned to each page's orientation, in the preview, the print job (with the spooler's media) and Save these pages as PDF; the document keeps its page size. `paper` param on `ui.print`/`file.printPdf` |
| 2026-10-14 | trivial | Pen / stylus row corrected: ink, eraser and Ink to Shape (with arrows) work; pressure and Ink to Math missing |
| 2026-10-13 | trivial | A window too big for its screen shrinks to fit when it opens (#1215): eframe sized the first window to the whole monitor, frame and taskbar not counted, so on a laptop screen its bottom, with the end of the scroll bar, was past the screen edge. A window saved that size is fitted on its next launch too |
| 2026-10-12 | trivial | 32-bit Windows starts on OpenGL: DirectX 12 graphics pipeline creation returns "The parameter is incorrect" and the app exited before the window appeared |
| 2026-10-12 | minor | Desktop File > Print now discovers printers and submits PDF jobs through pinned craft-print with printer choice, copies, duplex and grayscale; browser printing and PDF export remain available. The upstream preview, page ranges, parity and Print Markup remain available. Remaining print parity includes driver-specific tray/options and pages-per-sheet UI; physical printer fidelity needs hardware validation. |
| 2026-10-10 | trivial | Windows falls back to OpenGL when DirectX 12 offers only the software adapter (#316) |
| 2026-10-10 | trivial | Desktop File › Print opens the PDF in the system viewer (#286) |
| 2026-10-10 | trivial | Inertial touchpad scrolling landed (#252) |
| 2026-10-10 | major | First version |
