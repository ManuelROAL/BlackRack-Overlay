// Country flags and manufacturer logos ship as SVG, but every overlay draws
// them into boxes of about 17x12 and 20x20 CSS pixels. WebView2 still parses the
// whole vector document before it can rasterise those few pixels, and 31 of the
// 284 icons hold 2.2 MB of the 2.8 MB total: one 824 KB manufacturer logo is
// parsed to fill a 20 px square.
//
// This tool pre-rasterises them once, offline, at a size that still covers the
// largest supported combination of font expansion and display scale. The PNGs
// are committed so building the application needs neither this tool nor its
// `sharp` dependency, and the SVG sources stay in the repository as the master
// copies for any future size.
//
// Run with: npm run icons:raster
import fs from "node:fs";
import path from "node:path";
import sharp from "sharp";

const root = process.cwd();

// Overlays draw flags at 8.5 + 8.5 x 6 + 6 CSS px at most, and logos at
// 10 + 10 square. 48 px of height and 96 px of side keep a comfortable margin
// over a 150% display scale without rasterising anything anyone can see.
const groups = [
  { source: "src/assets/countries", target: "src/assets/countries-raster", height: 48 },
  { source: "src/assets/manufacturers", target: "src/assets/manufacturers-raster", side: 96 }
];

// A high render density keeps the vector crisp before it is scaled down; the
// aspect ratio is never changed here, so `object-fit` in CSS keeps deciding how
// each icon fills its box exactly as it does with the SVG sources.
const DENSITY = 300;

let written = 0;
let failed = 0;

for (const group of groups) {
  const sourceDirectory = path.join(root, group.source);
  const targetDirectory = path.join(root, group.target);
  fs.mkdirSync(targetDirectory, { recursive: true });

  const sources = fs.readdirSync(sourceDirectory)
    .filter((file) => /\.(svg|png)$/i.test(file))
    .sort();

  for (const file of sources) {
    const name = file.replace(/\.(svg|png)$/i, "");
    const target = path.join(targetDirectory, `${name}.png`);
    const resize = group.side === undefined
      ? { height: group.height, fit: "inside" }
      : { width: group.side, height: group.side, fit: "inside" };
    try {
      await sharp(path.join(sourceDirectory, file), { density: DENSITY })
        .resize({ ...resize, withoutEnlargement: false })
        .png({ compressionLevel: 9, palette: true })
        .toFile(target);
      written += 1;
    } catch (error) {
      console.error(`No se pudo rasterizar ${group.source}/${file}: ${error.message}`);
      failed += 1;
    }
  }
}

const directorySize = (directory) => fs.readdirSync(path.join(root, directory))
  .reduce((total, file) => total + fs.statSync(path.join(root, directory, file)).size, 0);

for (const group of groups) {
  const before = directorySize(group.source);
  const after = directorySize(group.target);
  console.log(
    `${group.target}: ${(after / 1024).toFixed(0)} KB `
    + `(desde ${(before / 1024).toFixed(0)} KB en ${group.source})`
  );
}
console.log(`${written} iconos rasterizados${failed > 0 ? `, ${failed} fallidos` : ""}`);
if (failed > 0) process.exitCode = 1;
