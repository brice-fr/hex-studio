# Screenshots

The images in the project README are generated, not captured by hand, so they
can be refreshed whenever the UI changes.

```bash
./docs/screenshots/capture.sh
```

`capture.sh` decodes the ASAM demo pair with the real Rust backend, renders the
app's own components against that data through `harness.svelte`, and captures
each scene with headless Chrome. The harness route is created under
`src/routes/__shots` and removed again, so nothing extra ships in the built app.

Captures are 1280×800 at 1× in the light theme, which keeps all three images
under half a megabyte and reads against GitHub's own light default. `SCALE=2`
gives crisp retina captures at roughly 2.4× the size; `THEME=dark` gives the
other theme.

The images therefore show **real decoded values** rather than mock-ups — the
numbers in them come from `ASAP2_Demo_V171.hex` read through
`ASAP2_Demo_V171.a2l`.

## Requirements

- The ASAM demo pair, which is ASAM-licensed and not vendored here. Point
  `A2L_DEMO_DIR` at a directory holding `ASAP2_Demo_V171.a2l` and
  `ASAP2_Demo_V171.hex` (defaults to `~/Downloads/ECU_Description`).
- Google Chrome, for `--headless --screenshot`. Override with `CHROME=`.
- A free port for the dev server; override with `PORT=`.
- Nothing else: there is no PNG optimiser in the pipeline. Chrome's output is
  already well filtered — recompressing losslessly gains about 2%, and some
  files grow — so it would be code for nothing.

## Scenes

| File | Shows |
|------|-------|
| `hex.png` | The hex view at the calibration block, with both side panes open — the rest of the image is `FF` padding and says nothing |
| `data.png` | The data view: coverage banner, categories, parameter table, and a curve with its plot |
| `og.png` | The repository's social preview card, 1280×640. Not referenced by the README — GitHub has no API for this, so it is uploaded by hand under Settings → General → Social preview |
| `map.png` | The map editor: shaded grid above the slice as a 3D surface. Uses the COM_AXIS map because its breakpoints are uneven and its values fold, so the surface shows shape and true spacing rather than a flat ramp |

Add a scene by extending `SCENES` in `harness.svelte` and the `for shot in …`
loop in `capture.sh`. A scene needing its own dimensions gets a case in that
loop's `size` switch, as `og` does.

## Social preview

`og.png` has to be uploaded manually: GitHub exposes no REST or GraphQL endpoint
for it, so re-running `capture.sh` refreshes the file but not the repository.
After a redesign, upload it again at **Settings → General → Social preview**.
