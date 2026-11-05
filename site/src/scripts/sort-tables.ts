// Sortable tables. Select a column heading to sort by it, and select it again to
// reverse the order. A table that reads across, not down, has `data-no-sort`.

type Value = number | string | null;

/** "17 of 28", "3/5", "83%", "80% to 99%" and "1,204" sort as numbers. */
function valueOf(cell: Element | undefined): Value {
  const text = (cell?.textContent ?? "").trim();
  if (!text || text === "–") return null;
  const part = text.match(/^(\d+)(?: of |\/)(\d+)/);
  if (part) return Number(part[2]) ? Number(part[1]) / Number(part[2]) : 0;
  const number = text.replaceAll(",", "").match(/^-?\d+(\.\d+)?(%|$| to )/);
  return number ? Number.parseFloat(text.replaceAll(",", "")) : text;
}

/** Empty cells stay last in both directions. */
function compare(a: Value, b: Value, direction: number): number {
  if (a === null || b === null) return Number(a === null) - Number(b === null);
  if (typeof a === "number" && typeof b === "number") return (a - b) * direction;
  return String(a).localeCompare(String(b), undefined, { numeric: true }) * direction;
}

const columnOf = (heading: HTMLTableCellElement) => Number(heading.dataset.col ?? heading.cellIndex);
const rowsOf = (body: HTMLTableSectionElement) => [...body.rows].filter((row) => !row.querySelector('[scope="rowgroup"]'));

function sortBy(heading: HTMLTableCellElement): void {
  const table = heading.closest("table");
  if (!table) return;
  const column = columnOf(heading);
  const first = valueOf(table.tBodies[0] && rowsOf(table.tBodies[0])[0]?.cells[column]);
  const opening = typeof first === "number" ? "descending" : "ascending";
  const current = heading.getAttribute("aria-sort");
  const next = current ? (current === "ascending" ? "descending" : "ascending") : opening;
  table.querySelectorAll("thead [aria-sort]").forEach((other) => other.removeAttribute("aria-sort"));
  heading.setAttribute("aria-sort", next);
  const direction = next === "ascending" ? 1 : -1;
  for (const body of table.tBodies) {
    rowsOf(body)
      .sort((a, b) => compare(valueOf(a.cells[column]), valueOf(b.cells[column]), direction))
      .forEach((row) => body.append(row));
  }
}

/** Add a sort button to each column heading that has something to sort. */
export function sortable(root: ParentNode): void {
  for (const table of root.querySelectorAll<HTMLTableElement>("table:not([data-no-sort])")) {
    const rows = [...table.tBodies].flatMap(rowsOf);
    if (!table.tHead || rows.length < 3) continue;
    for (const heading of table.tHead.querySelectorAll<HTMLTableCellElement>('th[scope="col"]')) {
      const column = columnOf(heading);
      if (heading.querySelector("button") || !heading.textContent?.trim()) continue;
      if (!rows.some((row) => row.cells[column]?.textContent?.trim())) continue;
      const button = document.createElement("button");
      button.type = "button";
      button.className = "sort";
      button.append(...heading.childNodes);
      heading.append(button);
    }
  }
}

document.addEventListener("click", (event) => {
  const button = (event.target as Element).closest("button.sort");
  const heading = button?.closest("th");
  if (heading) sortBy(heading);
});
