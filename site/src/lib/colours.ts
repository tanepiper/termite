/**
 * A family has one hue, and each model in it has its own shade of that hue. A model
 * and its copy with safety training removed share a shade; charts dash the copy.
 * Colour comes from `eval/families.jsonl`, so filters never repaint a model.
 */
import familyLines from "../../../eval/families.jsonl?raw";

export interface Family { family: string; slot: number; models: string[] }

export const familyList: Family[] = familyLines.trim().split("\n").map((line) => JSON.parse(line));

/** The site has this many family hues, checked for colour blindness in this order. */
const HUES = 12;
/** How far a shade may move from the family's hue, towards the page or the text. */
const LIGHTER = 40;
const DARKER = 40;

const baseName = (model: string) => model.replace(/, (safety training|refusals) removed$/, "");

/** Past the last hue, a family borrows one from a family that only rates commands. */
const BORROWED: Record<number, number> = { 13: 5, 14: 2, 15: 6 };

export const familyColour = (slot: number) => `var(--family-${BORROWED[slot] ?? (slot - 1) % HUES + 1})`;

export function familyOf(model: string): Family | undefined {
  const base = baseName(model);
  return familyList.find(({ models }) => models.some((name) => baseName(name) === base));
}

/** This model's shade. The middle of a family stays on the family's hue. */
export function modelColour(model: string): string {
  const family = familyOf(model);
  if (!family) return "var(--ink-soft)";
  const hue = familyColour(family.slot);
  const bases = [...new Set(family.models.map(baseName))].sort();
  if (bases.length < 2) return hue;
  const place = (2 * bases.indexOf(baseName(model))) / (bases.length - 1) - 1;
  if (place === 0) return hue;
  const [towards, reach] = place < 0 ? ["var(--surface)", LIGHTER] : ["var(--ink)", DARKER];
  return `color-mix(in oklab, ${hue}, ${towards} ${Math.round(Math.abs(place) * reach)}%)`;
}
