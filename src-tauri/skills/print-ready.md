<print>
- Target resolution: the project's target dpi (default 100) at physical size. An image layer's natural px / (w_mm / 25.4) below the requirement makes export_check warn. Backgrounds (strategy=background) need half the target. Fix warnings with extend_background(height_px up to 2160) or generate_image at a larger size, or tell the user plainly why it cannot be fixed.
- Strokes and dots thinner than 1 mm vanish in large-format print. Text outlines are at least 2 mm.
- Legibility: if a letter looks thin, or an outline fills the counters, look at the original again and reduce the stroke width.
- Colors stay RGB. Mention in one line that neon colors may print duller.
</print>
