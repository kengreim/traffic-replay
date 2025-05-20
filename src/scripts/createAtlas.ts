import sharp from "sharp";
import { mkdirSync, readdirSync, writeFileSync } from "fs";
import { basename, join } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = join(__filename, "../../");

const ICONS_DIR = join(__dirname, "assets", "icons");
const OUTPUT_DIR = join(__dirname, "../", "public");
const DATA_DIR = join(__dirname, "data");
const ICONS_PER_ROW = 10;

interface IconMapping {
  [key: string]: {
    x: number;
    y: number;
    width: number;
    height: number;
    mask: boolean;
    anchorX: number;
    anchorY: number;
  };
}

interface Composite {
  input: string;
  top: number;
  left: number;
}

interface IconInfo {
  file: string;
  width: number;
  height: number;
}

async function createAtlas(): Promise<void> {
  try {
    // Get all PNG files and their dimensions
    const files: string[] = readdirSync(ICONS_DIR).filter((file: string) => file.endsWith(".png"));
    const iconInfos: IconInfo[] = [];

    // Get dimensions for all icons
    for (const file of files) {
      const filePath = join(ICONS_DIR, file);
      const metadata = await sharp(filePath).metadata();
      if (metadata.width && metadata.height) {
        iconInfos.push({
          file,
          width: metadata.width,
          height: metadata.height,
        });
      }
    }

    // Sort icons by height to optimize space usage
    iconInfos.sort((a, b) => b.height - a.height);

    // Calculate atlas dimensions
    //const rows = Math.ceil(iconInfos.length / ICONS_PER_ROW);
    const maxWidth = Math.max(...iconInfos.map((icon) => icon.width));
    const atlasWidth = ICONS_PER_ROW * maxWidth;

    // Calculate total height needed
    let currentY = 0;
    let currentX = 0;
    let maxHeightInRow = 0;
    let rowCount = 0;

    // Create icon mapping
    const iconMapping: IconMapping = {};
    const composites: Composite[] = [];

    for (const iconInfo of iconInfos) {
      const filePath = join(ICONS_DIR, iconInfo.file);
      const aircraftType = basename(iconInfo.file, ".png");

      // Check if we need to move to next row
      if (currentX + iconInfo.width > atlasWidth) {
        currentX = 0;
        currentY += maxHeightInRow;
        maxHeightInRow = 0;
        rowCount++;
      }

      // Update max height in current row
      maxHeightInRow = Math.max(maxHeightInRow, iconInfo.height);

      iconMapping[aircraftType] = {
        x: currentX,
        y: currentY,
        width: iconInfo.width,
        height: iconInfo.height,
        mask: true,
        anchorX: iconInfo.width / 2,
        anchorY: iconInfo.height / 2,
      };

      composites.push({
        input: filePath,
        top: currentY,
        left: currentX,
      });

      currentX += iconInfo.width;
    }

    // Calculate final atlas height
    const atlasHeight = currentY + maxHeightInRow;

    // Create a blank canvas
    const atlas = sharp({
      create: {
        width: atlasWidth,
        height: atlasHeight,
        channels: 4,
        background: { r: 0, g: 0, b: 0, alpha: 0 },
      },
    });

    // Save the atlas
    await atlas.composite(composites).png().toFile(join(OUTPUT_DIR, "atlas.png"));

    // Save the mapping
    writeFileSync(join(OUTPUT_DIR, "iconMapping.json"), JSON.stringify(iconMapping, null, 2));

    // Create aircraft list with scale factors
    const aircraftScales: { [key: string]: number } = {};

    // Find the smallest icon dimensions
    const minWidth = Math.min(...iconInfos.map((icon) => icon.width));
    const minHeight = Math.min(...iconInfos.map((icon) => icon.height));
    const minDimension = Math.min(minWidth, minHeight);

    // Calculate scale factors for each aircraft
    for (const icon of iconInfos) {
      const aircraftType = basename(icon.file, ".png");
      aircraftScales[aircraftType] = Math.min(icon.width, icon.height) / minDimension;
    }

    // Ensure data directory exists
    try {
      mkdirSync(DATA_DIR, { recursive: true });
    } catch (error) {
      // Directory might already exist, which is fine
    }

    // Save aircraft scales
    writeFileSync(join(DATA_DIR, "aircraft.json"), JSON.stringify(aircraftScales, null, 2));

    console.log("Atlas, mapping, and aircraft scales created successfully!");
  } catch (error) {
    console.error("Error creating atlas:", error);
    throw error;
  }
}

createAtlas().catch(console.error);
