# Pretendard fonts

Drop Pretendard `.woff2` files here, for example:

- `PretendardVariable.woff2` (one file, all weights), or
- `Pretendard-Regular.woff2`, `Pretendard-Medium.woff2`, `Pretendard-SemiBold.woff2`, `Pretendard-Bold.woff2`

Download: https://github.com/orioncactus/pretendard/releases (`web/static/woff2` or `web/variable/woff2`).

`vite.config.ts` scans this folder when it serves or builds `index.html` and adds an `@font-face` rule for each file it finds. Weights come from the file name suffix. If the folder has no fonts, the app uses `Pretendard` / `Pretendard Variable` when installed on the system, and `system-ui` otherwise. Restart `pnpm dev` after adding files.
