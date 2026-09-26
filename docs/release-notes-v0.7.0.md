# Release notes 0.7.0

mojiroku 0.7.0 refreshes the interface, adds automatic titles, and makes long
recordings easier to review and to stop.

- **Automatic titles:** after a microphone recording or a meeting is transcribed, a
  default name such as 「録音」 or 「会議」 is replaced with a short title based on the
  conversation. This uses the local summary model only when it is already downloaded.
  Nothing is sent to the cloud and no download starts. Imported files keep their file
  names, and titles you typed or cleared are never replaced automatically. The ✨ button next to a
  recording's title generates one on demand. It follows your summary engine setting and
  asks before sending the transcript to a cloud provider.
- **Light and dark appearance:** the app follows the macOS setting, with more readable
  text contrast and a consistent type scale.
- **Easier review:** AI minutes are shown as formatted text. Clicking a line's timestamp
  plays the audio from there, and the current line is highlighted as it plays. You can
  change the playback speed, skip back and forward, and search the transcript.
- **Stop running jobs:** transcription and speaker separation can now be stopped while
  they run. The recording keeps its previous transcript and speakers.
- **Recording feedback:** the recording screens show the real input level and warn when
  no sound is being picked up. A microphone recording can be discarded.
- **Clearer state:** History and the sidebar show whether each recording is processing,
  failed or not yet transcribed, and the recording screen offers only the actions that
  apply. The first launch explains what will be downloaded. A meeting started without
  the transcription model now says that live captions will not appear.
- **Re-run speaker separation:** any transcribed recording, including meetings, can be
  separated again. This corrects recordings that earlier versions split into too many
  speakers.
- **Fewer extra speakers:** short backchannels such as 「はい」 no longer create extra
  speakers. In a one-on-one meeting that 0.6.2 split into 8 remote speakers, the other
  person is now 1 speaker.
- **Smaller summary model for Macs under 16 GB:** new installs download Qwen3.5-4B
  (2.7 GB) instead of Qwen2.5-7B. A model you already have keeps being used, and 7B stays
  selectable in Settings. Japanese summaries no longer contain simplified Chinese
  characters.

Existing recordings are not renamed or re-separated automatically. Automatic titles can
occasionally include a mis-heard name; rename the recording if so. Speaker recognition is
still approximate. No database migration is required.
