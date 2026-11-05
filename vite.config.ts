import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import process from "node:process";
import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
const host = process.env.TAURI_DEV_HOST;

const WEIGHTS: Record<string, string> = {
  Thin: "100",
  ExtraLight: "200",
  Light: "300",
  Regular: "400",
  Medium: "500",
  SemiBold: "600",
  Bold: "700",
  ExtraBold: "800",
  Black: "900",
};

// Emits @font-face only for Pretendard files actually present, so a missing font never 404s.
function pretendard(): Plugin {
  return {
    name: "pretendard-fonts",
    transformIndexHtml() {
      const dir = "public/fonts";
      if (!existsSync(dir)) return [];
      const files: string[] = readdirSync(dir).filter((f: string) => f.endsWith(".woff2"));
      const css = files
        .map((f) => {
          const weight = /variable/i.test(f)
            ? "45 920"
            : (WEIGHTS[
                f
                  .replace(/\.woff2$/, "")
                  .split("-")
                  .pop() ?? ""
              ] ?? "400");
          return `@font-face{font-family:Pretendard;src:url(/fonts/${f}) format("woff2");font-weight:${weight};font-display:swap}`;
        })
        .join("");
      return css ? [{ tag: "style", children: css, injectTo: "head" }] : [];
    },
  };
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), pretendard()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      // target은 cargo가 계속 쓰는 폴더라 감시하면 이벤트 폭주로 dev 서버가 멈춤
      ignored: ["**/src-tauri/**", "**/target/**", "**/installer/**", "**/dist-installer/**"],
    },
  },
}));
