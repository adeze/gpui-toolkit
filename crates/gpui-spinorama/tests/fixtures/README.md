# Spinorama API fixtures

Real KEF R3 / `asr` measurements captured from the Spinorama API on 2026-10-13.

Source: `https://api.spinorama.org/v1/speaker/KEF%20R3/version/asr/measurements/{measurement}?measurement_format=json`

The small JSON fixtures retain the six CEA2034 traces and the first horizontal
contour trace, without Plotly layout or annotation data. The gzip bundle retains
the API wrappers for CEA2034, SPL Horizontal, SPL Vertical, and SPL Horizontal
Contour. Browser QA intercepts requests with that bundle so all eight views and
3D interactions can be checked without network variability. Live API requests
are validated separately with `qa_spinorama_browser.py --live-api`.
