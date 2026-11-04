const chunk = (files, size = 40) =>
  Array.from({ length: Math.ceil(files.length / size) }, (_, i) => files.slice(i * size, (i + 1) * size));

// Windows 명령줄 8191자 제한: 파일을 40개씩 나눠 실행
const run = (cmd) => (files) =>
  chunk(files).map((c) => `${cmd} ${c.map((f) => JSON.stringify(f)).join(" ")}`);

export default {
  "*.{ts,tsx,js,css,json,md,yml,yaml}": run("prettier --write"),
  "*.{ts,tsx}": run("eslint --fix"),
  "*.rs": run("rustfmt --edition 2024"),
};
