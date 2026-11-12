import { IMAGE_FILE } from "./format.ts";

for (const name of ["a.png", "a.PNG", "a.jpg", "a.jpeg", "a.webp", "a.bmp", "a.gif", "a.tif", "a.TIFF"]) {
  if (!IMAGE_FILE.test(name)) throw new Error(`${name} should be accepted`);
}
for (const name of ["a.pdf", "a.svg", "a.tif.txt", "tiff"]) {
  if (IMAGE_FILE.test(name)) throw new Error(`${name} should be rejected`);
}

console.log("format: ok");
