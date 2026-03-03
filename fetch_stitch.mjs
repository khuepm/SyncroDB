import fs from 'fs';
import path from 'path';

const API_KEY = process.env.STITCH_API_KEY; // Replace with your actual key or use .env file
const PROJECT_ID = "16266398012239050806";
const SCREENS = [
  "2489cef2db7a4792842201db32401ac6",
  "211e6d8b1b36463e9459aecab39d2fdd",
  "6f2b3588029e4a04a69fbe62048212d7",
  "739edf22cd634ea386520ab29d82f0e2",
  "1a03b6e8f677482d82aff79a881979ca",
  "35efd88fbf4843c5a86f3649cdb1426e",
  "3dff909ac6c848998e18f4ab38814a22"
];

const BASE_URL = `https://stitch.googleapis.com/v1/projects/${PROJECT_ID}/screens`;
const ASSETS_DIR = path.join(process.cwd(), "stitch_assets");

if (!fs.existsSync(ASSETS_DIR)) {
  fs.mkdirSync(ASSETS_DIR, { recursive: true });
}

async function downloadFile(url, dest) {
  const res = await fetch(url);
  const buffer = await res.arrayBuffer();
  fs.writeFileSync(dest, Buffer.from(buffer));
}

async function main() {
  for (const id of SCREENS) {
    console.log(`Fetching screen info for ${id}...`);
    const res = await fetch(`${BASE_URL}/${id}`, {
      headers: { "X-Goog-Api-Key": API_KEY }
    });

    if (!res.ok) {
      console.error(`Failed to fetch ${id}: ${res.statusText}`);
      continue;
    }

    const data = await res.json();
    const title = data.title.replace(/[^a-zA-Z0-9-_]/g, '_');

    if (data.htmlCode && data.htmlCode.downloadUrl) {
      console.log(`Downloading HTML for ${title}...`);
      await downloadFile(data.htmlCode.downloadUrl, path.join(ASSETS_DIR, `${title}.html`));
    }

    if (data.screenshot && data.screenshot.downloadUrl) {
      console.log(`Downloading Screenshot for ${title}...`);
      await downloadFile(data.screenshot.downloadUrl, path.join(ASSETS_DIR, `${title}.jpg`));
    }
  }
  console.log("Done fetching all screens.");
}

main().catch(console.error);
