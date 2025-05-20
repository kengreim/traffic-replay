import sharp from "sharp";
import { readdirSync, writeFileSync } from "fs";
import { join, basename } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = join(__filename, "../../");

const ICONS_DIR = join(__dirname, "assets", "icons");
const OUTPUT_DIR = join(__dirname, "assets");
const ICON_SIZE = 128;
const ICONS_PER_ROW = 10;

interface IconMapping {
  [key: string]: {
    x: number;
    y: number;
    width: number;
    height: number;
    mask: boolean;
  };
}

interface Composite {
  input: string;
  top: number;
  left: number;
}

async function createAtlas(): Promise<void> {
  try {
    // Get all PNG files
    const files: string[] = readdirSync(ICONS_DIR).filter((file: string) => file.endsWith(".png"));

    // Calculate atlas dimensions
    const rows = Math.ceil(files.length / ICONS_PER_ROW);
    const atlasWidth = ICONS_PER_ROW * ICON_SIZE;
    const atlasHeight = rows * ICON_SIZE;

    // Create a blank canvas
    const atlas = sharp({
      create: {
        width: atlasWidth,
        height: atlasHeight,
        channels: 4,
        background: { r: 0, g: 0, b: 0, alpha: 0 },
      },
    });

    // Create icon mapping
    const iconMapping: IconMapping = {};
    let x = 0;
    let y = 0;

    // Composite each icon onto the atlas
    const composites: Composite[] = [];
    for (const file of files) {
      const aircraftType = basename(file, ".png");
      iconMapping[aircraftType] = {
        x,
        y,
        width: ICON_SIZE,
        height: ICON_SIZE,
        mask: true,
      };

      composites.push({
        input: join(ICONS_DIR, file),
        top: y,
        left: x,
      });

      x += ICON_SIZE;
      if (x >= atlasWidth) {
        x = 0;
        y += ICON_SIZE;
      }
    }

    // Save the atlas
    await atlas.composite(composites).png().toFile(join(OUTPUT_DIR, "atlas.png"));

    // Save the mapping
    writeFileSync(join(OUTPUT_DIR, "iconMapping.json"), JSON.stringify(iconMapping, null, 2));

    console.log("Atlas and mapping created successfully!");
  } catch (error) {
    console.error("Error creating atlas:", error);
    throw error;
  }
}

createAtlas().catch(console.error);
