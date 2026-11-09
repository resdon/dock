# Popup SHM buffer length fix

## Symptom

Opening the Rclone tray popup could panic at `copy_from_slice` because the
rendered pixel vector is tightly sized to `width * height * 4`, while
`smithay_client_toolkit::shm::slot::SlotPool::create_buffer` can return a canvas
slice with a slightly larger allocation (for example, 270016 bytes for a
270000-byte image payload).

## Fix

A shared `app::copy_popup_content` helper now:

1. Checks that the destination canvas can hold the complete rendered image.
2. Copies the pixel payload into the beginning of the SHM canvas.
3. Zeroes any allocator padding after the pixel payload.
4. Returns `false` rather than panicking if the destination is unexpectedly
   smaller than the image.

The helper is used by the Rclone, process, audio, Bluetooth, context-menu, and
window-list popup paths. This prevents the same exact-length assumption from
causing the next popup to panic.

Unit tests cover both an allocator-padded destination and an undersized
canvas. Run with:

```bash
cargo test --lib app::popup_buffer_tests
```
