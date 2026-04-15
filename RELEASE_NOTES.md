# Moxin Voice v0.0.3.beta.3

## Highlights

- Added **Live Translation** with real-time bilingual subtitles from either microphone or system audio.
- Improved subtitle chunking and display pairing for a more stable live translation experience.
- Improved microphone input compatibility by opening capture streams with device-native formats before converting to the app's internal 16 kHz mono pipeline.
- Removed the obsolete translation merge option from the Live Translation UI.

## Notes

- Live Translation system audio input is available on macOS through ScreenCaptureKit.
- Microphone and system audio permissions may be required on first use, depending on the selected input source.
