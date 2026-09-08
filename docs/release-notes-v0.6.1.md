# Release notes 0.6.1

mojiroku 0.6.1 improves model preparation and keeps completed meeting translations available after recording.

- **Prepare models before a meeting:** use Download in Settings for transcription, summary, translation, and missing live-caption models. Selecting a model alone does not start a download.
- **Keep download progress:** changing pages no longer cancels model downloads. Interrupted catalog downloads retain their partial files and can resume when requested again.
- **Keep completed translations:** navigating, turning translation off, or changing the target language preserves completed results. Stopping and saving a meeting stores them in the recording's Saved translations tab, alongside the exact original live captions.
- **Clearer meeting screen:** repeated explanatory text is reduced, with more detail available when needed. Model download controls use the interface language.
- **Japanese/English Auto detection:** live and offline automatic recognition now chooses between Japanese and English, including mixed-mode windows. Brief or unclear speech can still be recognized incorrectly.
- **Better handling of brief speakers:** speaker cleanup retains distinct short contributions instead of merging them solely because they are brief. Speaker labels can still need correction.

Only translations completed when recording stops are saved. Pending translations are not saved, and translations lost in older versions cannot be recovered. Quitting before saving can still lose the current translation history.

Live translation still requires at least 16 GB RAM and its separate 5.68 GB model download. The recording database upgrades to schema 7 to store translations; the bundled MCP server is updated with it. Broader accuracy verification continues through real meeting use.
