// The explorer. Everything comes from observation rows
// (eval/schema/observation.md): filter them, draw them, click to see them.

type Row = {
  started: number; commit: string; corpus: string; suite: string; model: string; family: string;
  model_kind: string; size_b: number; refusals_removed: boolean; case: string; level: number;
  domain: string; technique: string; query_kind: string; repeat: number; fill: number;
  ending: string; wrote_hazard: boolean; failure: string; stage: string; seconds: number;
  answer: string; tier: number | null; harmful: number | null; policy_allows: boolean | null;
};

/** Colours that say which model. They never say good or bad. */
const COLOURS = 6;
const FAILURE_WORDS: Record<string, string> = {
  policy_gap: "Got past the rules", straight_in_risky: "Risky, straight to the prompt",
  injection_followed: "Fell for hidden instructions", understated: "Called a dangerous command safe",
  hazard_written: "Wrote something dangerous", wrong_answer: "Wrong answer", no_command: "No command",
  timeout: "Did not complete in time", context_overflow: "Too much to read at once", malformed_reply: "Broken reply",
  server_error: "Server error", rated_safe: "Called a dangerous command safe",
};
/** What went wrong, worst first. Position in this list picks the colour, `--seg-N`. */
const SEVERITY: { label: string; modes: string[] }[] = [
  { label: "Got past the rules", modes: ["policy_gap"] },
  { label: "Risky, straight to the prompt", modes: ["straight_in_risky"] },
  { label: "Fell for hidden instructions", modes: ["injection_followed"] },
  { label: "Wrote something dangerous", modes: ["hazard_written"] },
  { label: "Called a dangerous command safe", modes: ["understated", "rated_safe"] },
  { label: "Wrong answer", modes: ["wrong_answer"] },
  { label: "No command", modes: ["no_command"] },
  { label: "Did not complete in time", modes: ["timeout"] },
  { label: "Did not complete: an error", modes: ["context_overflow", "malformed_reply", "server_error"] },
];
const TABS: [string, string][] = [
  ["text", "Command models"], ["decision", "Decision models"], ["vision", "Vision models"], ["image", "Image models"],
];
const FACETS: [keyof Row, string][] = [
  ["suite", "Tests"], ["model", "Model"], ["family", "Family"], ["query_kind", "Intent"],
  ["domain", "Area"], ["technique", "Type"], ["level", "Level"], ["failure", "Failure"], ["fill", "How full its memory was (%)"],
];

const $ = (id: string) => document.getElementById(id) as HTMLElement;
const esc = (text: unknown) => String(text).replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const share = (part: number, whole: number) => (whole ? Math.round((100 * part) / whole) : 0);
const sortedSet = <T,>(items: T[]) => [...new Set(items)].sort((a, b) => (a < b ? -1 : 1));
const groupBy = <T,>(items: T[], key: (item: T) => string) => {
  const groups = new Map<string, T[]>();
  for (const item of items) groups.set(key(item), [...(groups.get(key(item)) ?? []), item]);
  return groups;
};

let rows: Row[] = [];
let kind = "text";
const chosen: Partial<Record<keyof Row, string>> = {};
const drills = new Map<string, { title: string; rows: Row[] }>();

const baseName = (model: string) => model.replace(/, safety training removed$/, "");
const removed = (model: string) => baseName(model) !== model;

/** A model keeps its colour whatever the filters do, and shares it with its stripped copy. */
function colour(model: string): string {
  const names = sortedSet(rows.filter((row) => row.model_kind === kind).map((row) => baseName(row.model)));
  const slot = names.indexOf(baseName(model)) + 1;
  return slot <= COLOURS ? `var(--family-${slot})` : "var(--ink-soft)";
}

function drillId(title: string, picked: Row[]): string {
  const id = `d${drills.size}`;
  drills.set(id, { title, rows: picked });
  return id;
}

function visible(): Row[] {
  return rows.filter((row) =>
    row.model_kind === kind &&
    FACETS.every(([key]) => !chosen[key] || String(row[key]) === chosen[key]));
}

function legend(models: string[]): string {
  const swatch = (m: string) => `<i class="${removed(m) ? "dashed" : ""}" style="--swatch:${colour(m)}"></i>`;
  return `<p class="legend">${models.map((m) => `<span>${swatch(m)}${esc(m)}</span>`).join("")}</p>`;
}

/** The figures behind a chart, so nobody has to hover to read them. */
function numbers(corner: string, columns: string[], body: [string, string[]][]): string {
  const head = [corner, ...columns].map((c, i) => `<th scope="col" class="${i ? "number" : ""}">${esc(c)}</th>`).join("");
  const lines = body.map(([name, cells]) =>
    `<tr><th scope="row">${esc(name)}</th>${cells.map((c) => `<td class="number">${c}</td>`).join("")}</tr>`).join("");
  return `<div class="scroll"><table class="figures"><thead><tr>${head}</tr></thead><tbody>${lines}</tbody></table></div>`;
}

function section(title: string, note: string, body: string): string {
  return `<section class="view"><h2>${esc(title)}</h2><p class="note">${note}</p>${body}</section>`;
}

/** Lines on one percentage axis. `points` is series → (x position, value, rows). */
function lines(labels: string[], series: Map<string, { x: number; value: number; rows: Row[]; label: string }[]>): string {
  const [w, h, left, bottom] = [900, 240, 34, 24];
  const x = (i: number) => left + (i * (w - left - 14)) / Math.max(1, labels.length - 1);
  const y = (v: number) => 8 + ((100 - v) * (h - bottom - 8)) / 100;
  let svg = `<svg viewBox="0 0 ${w} ${h}" role="img">`;
  for (const tick of [0, 50, 100]) {
    svg += `<line x1="${left}" x2="${w - 14}" y1="${y(tick)}" y2="${y(tick)}" stroke="var(--line)"/><text x="${left - 6}" y="${y(tick) + 4}" text-anchor="end">${tick}</text>`;
  }
  labels.forEach((label, i) => {
    if (labels.length <= 12 || i % Math.ceil(labels.length / 10) === 0) {
      svg += `<text x="${x(i)}" y="${h - 6}" text-anchor="middle">${esc(label)}</text>`;
    }
  });
  for (const [model, points] of series) {
    const c = colour(model);
    const dash = removed(model) ? ' stroke-dasharray="6 4"' : "";
    svg += `<polyline fill="none" stroke="${c}" stroke-width="2"${dash} points="${points.map((p) => `${x(p.x)},${y(p.value)}`).join(" ")}"/>`;
    for (const p of points) {
      const id = drillId(`${model}, ${p.label}`, p.rows);
      svg += `<circle data-drill="${id}" cx="${x(p.x)}" cy="${y(p.value)}" r="5" fill="${c}" stroke="var(--surface)" stroke-width="2"><title>${esc(model)}, ${esc(p.label)}: ${p.value}% of ${p.rows.length}</title></circle>`;
    }
  }
  const body: [string, string[]][] = [...series.entries()].map(([model, points]) => [model, labels.map((_, i) => {
    const point = points.find((p) => p.x === i);
    return point ? `${point.value}%` : "–";
  })]);
  return `${svg}</svg>${legend([...series.keys()])}${numbers("Model", labels, body)}`;
}

/** Columns get wider as the history grows, so the table never gets long. */
function periodOf(runs: number[]): (t: number) => string {
  const days = (runs[runs.length - 1] - runs[0]) / 86400;
  const iso = (t: number) => new Date(t * 1000).toISOString();
  if (days <= 10) return (t) => iso(t).slice(5, 10);
  if (days > 70) return (t) => iso(t).slice(0, 7);
  return (t) => {
    const daysSinceMonday = (new Date(t * 1000).getUTCDay() + 6) % 7;
    return `Week of ${iso(t - daysSinceMonday * 86400).slice(5, 10)}`;
  };
}

/**
 * Over time: models appear when first tested and stop when dropped. One test
 * at a time, because different tests ask different things: a line across them
 * would show the tests changing, not the models. A column is a period, and
 * each model's figure is its last run in that period.
 */
function overTime(data: Row[]): string {
  const test = chosen.suite ?? (data.some((row) => row.suite === "core") ? "core" : data[0]?.suite);
  const mine = data.filter((row) => row.suite === test);
  const runs = sortedSet(mine.map((row) => row.started));
  const period = runs.length ? periodOf(runs) : () => "";
  const labels = [...new Set(runs.map(period))];
  if (labels.length < 2) return section("Over time", "All of these runs are in one period, so there's no line to draw yet. The next day's run adds a point.", lines(labels, seriesOf(mine, labels, period)));
  const note = `Share of requests where the model wrote something hazardous, in the "${esc(test)}" test. Pick another under Tests. Each column shows a model's last run in that period. Only compare runs on the same test set.`;
  return section("Over time", note, lines(labels, seriesOf(mine, labels, period)));
}

function seriesOf(data: Row[], labels: string[], period: (t: number) => string) {
  const series = new Map();
  for (const [model, rowsOf] of groupBy(data, (row) => row.model)) {
    series.set(model, labels.flatMap((label, x) => {
      const inPeriod = rowsOf.filter((row) => period(row.started) === label);
      if (!inPeriod.length) return [];
      const last = Math.max(...inPeriod.map((row) => row.started));
      const inRun = inPeriod.filter((row) => row.started === last);
      return [{ x, rows: inRun, label, value: share(inRun.filter((r) => r.wrote_hazard).length, inRun.length) }];
    }));
  }
  return series;
}

/** Does it give the same answer twice? */
function reliability(data: Row[]): string {
  const repeated = data.filter((row) => row.suite === "trial" && row.fill === 0);
  if (!repeated.length) return section("Same question, same answer?", "No repeat trials in this selection.", "");
  const models = sortedSet(repeated.map((row) => row.model));
  const cases = sortedSet(repeated.map((row) => row.case));
  const cells = models.map((model) => {
    const tds = cases.map((id) => {
      const mine = repeated.filter((row) => row.model === model && row.case === id);
      if (!mine.length) return "<td></td>";
      const counts = [...groupBy(mine, (row) => row.answer || row.ending).values()].map((g) => g.length);
      const same = share(Math.max(...counts), mine.length);
      const unsteady = 100 - same;
      const id2 = drillId(`${model}, ${id}: ${counts.length} different answers in ${mine.length}`, mine);
      return `<td data-drill="${id2}" title="${esc(id)}: ${same}% gave the same answer" style="background:color-mix(in oklab,var(--fail-3) ${unsteady}%,var(--panel))">${counts.length}</td>`;
    }).join("");
    const groups = [...groupBy(repeated.filter((row) => row.model === model), (row) => row.case).values()];
    const steady = groups.filter((g) => new Set(g.map((r) => r.answer || r.ending)).size === 1).length;
    return `<tr><th scope="row">${esc(model)}</th><th class="number">${steady} of ${groups.length}</th>${tds}</tr>`;
  }).join("");
  return section("Same question, same answer?",
    "Each cell is one request, and the number is how many different answers the model gave across its repeats. 1 is steady, and redder is less steady. The first column counts the requests that always got the same answer.",
    `<div class="scroll"><table class="grid"><thead><tr><td></td><th scope="col" class="number">Steady</th><th scope="colgroup" colspan="${cases.length}">Different answers, request by request</th></tr></thead><tbody>${cells}</tbody></table></div>`);
}

/** What fills up first as the context does. */
function flood(data: Row[]): string {
  const trials = data.filter((row) => row.suite === "trial");
  const fills = sortedSet(trials.map((row) => row.fill)).sort((a, b) => a - b);
  if (fills.length < 2) return section("When the context fills up", "No flood trials in this selection.", "");
  const series = new Map();
  for (const [model, mine] of groupBy(trials, (row) => row.model)) {
    series.set(model, fills.flatMap((fill, x) => {
      const at = mine.filter((row) => row.fill === fill);
      if (!at.length) return [];
      const bad = at.filter((r) => r.failure !== "none" && r.failure !== "hazard_written");
      return [{ x, rows: at, label: `${fill}% full`, value: share(bad.length, at.length) }];
    }));
  }
  return section("When the context fills up",
    "Share of trials that went wrong (not counting plain hazardous answers) as a long session piles up uncleared. 100 is the model's limit, so past that something has to give.",
    lines(fills.map((f) => `${f}%`), series));
}

/** How it failed, by model. A full bar is all that model's trials. */
function failures(data: Row[]): string {
  const shown = SEVERITY.map((level, index) => ({ ...level, slot: index + 1 }))
    .filter((level) => data.some((row) => level.modes.includes(row.failure)));
  const models = [...groupBy(data, (row) => row.model).entries()]
    .map(([model, mine]) => ({ model, mine, bad: mine.filter((row) => row.failure !== "none").length }))
    .sort((a, b) => b.bad / b.mine.length - a.bad / a.mine.length);
  const hits = (mine: Row[], modes: string[]) => mine.filter((row) => modes.includes(row.failure));
  const bars = models.map(({ model, mine, bad }) => {
    const parts = shown.map((level) => {
      const hit = hits(mine, level.modes);
      if (!hit.length) return "";
      const width = (100 * hit.length) / mine.length;
      const id = drillId(`${model}: ${level.label}`, hit);
      return `<i class="f${level.slot}" data-drill="${id}" title="${esc(level.label)}: ${hit.length} of ${mine.length}" style="width:${width}%">${width >= 5 ? hit.length : ""}</i>`;
    }).join("");
    return `<div class="fail"><span>${esc(model)}</span><span class="fbar">${parts}</span><span class="number">${bad} of ${mine.length}<small>${share(bad, mine.length)}%</small></span></div>`;
  }).join("");
  const key = shown.map((level) => `<span><i class="f${level.slot}"></i>${esc(level.label)} (${hits(data, level.modes).length})</span>`).join("");
  const body: [string, string[]][] = models.map(({ model, mine, bad }) => [model, [...shown.map((level) => String(hits(mine, level.modes).length)), `${bad} of ${mine.length}`]]);
  return section("How it went wrong", "The whole bar is every trial for that model, and the coloured part is the trials that went wrong, by their worst failure. Reds, purple and oranges are dangerous, blues are wrong or missing answers, and greys did not finish. Click a segment for the rows.",
    `${bars}<p class="legend">${key}</p>${numbers("Model", [...shown.map((level) => level.label), "All failures"], body)}`);
}

/** A decision model: score against tier, one dot per rating. */
function ladder(data: Row[]): string {
  const [w, h, left, bottom] = [900, 300, 36, 26];
  const x = (tier: number) => left + 40 + (tier * (w - left - 80)) / 6;
  const y = (v: number) => 8 + (1 - v) * (h - bottom - 8);
  let svg = `<svg viewBox="0 0 ${w} ${h}" role="img" aria-label="Harm score for each ladder rating, by tier">`;
  for (const t of [0, 0.5, 1]) svg += `<line x1="${left}" x2="${w - 10}" y1="${y(t)}" y2="${y(t)}" stroke="var(--line)"/><text x="${left - 6}" y="${y(t) + 4}" text-anchor="end">${t}</text>`;
  for (let tier = 0; tier <= 6; tier++) svg += `<text x="${x(tier)}" y="${h - 6}" text-anchor="middle">Tier ${tier}</text>`;
  const models = sortedSet(data.map((row) => row.model));
  data.forEach((row, i) => {
    const offset = (models.indexOf(row.model) - (models.length - 1) / 2) * 34 + ((i % 5) - 2) * 4;
    const id = drillId(`${row.model}, ${row.case}`, [row]);
    svg += `<circle data-drill="${id}" cx="${x(row.tier ?? 0) + offset}" cy="${y(row.harmful ?? 0)}" r="4" fill="${colour(row.model)}" fill-opacity="0.75"><title>${esc(row.model)} ${esc(row.case)}: ${(row.harmful ?? 0).toFixed(2)}</title></circle>`;
  });
  const tiers = [0, 1, 2, 3, 4, 5, 6];
  const body: [string, string[]][] = models.map((model) => [model, tiers.map((tier) => {
    const scores = data.filter((row) => row.model === model && row.tier === tier).map((row) => row.harmful ?? 0);
    return scores.length ? (scores.reduce((a, b) => a + b, 0) / scores.length).toFixed(2) : "–";
  })]);
  return section("Does the score follow the ladder?",
    "Each dot is one rating. A good decision model climbs from bottom left to top right. The table gives the average score at each tier.",
    `${svg}</svg>${legend(models)}${numbers("Model", tiers.map((tier) => `Tier ${tier}`), body)}`);
}

function draw(): void {
  drills.clear();
  const data = visible();
  $("count").textContent = `${data.length} of ${rows.filter((row) => row.model_kind === kind).length} trials`;
  const views = $("views");
  if (!data.length) {
    views.innerHTML = `<p>No trials ${rows.some((row) => row.model_kind === kind) ? "match these filters" : "for this kind of model yet. The schema has a place for them, the tests don't exist"}.</p>`;
  } else if (kind === "decision") {
    views.innerHTML = overTime(data) + ladder(data) + failures(data);
  } else {
    views.innerHTML = failures(data) + reliability(data) + flood(data) + overTime(data);
  }
  $("drill").hidden = true;
}

function drawFacets(): void {
  const mine = rows.filter((row) => row.model_kind === kind);
  $("facets").innerHTML = FACETS.map(([key, label]) => {
    const values = sortedSet(mine.map((row) => String(row[key])));
    if (values.length < 2) return "";
    const options = values.map((v) => `<option value="${esc(v)}" ${chosen[key] === v ? "selected" : ""}>${esc(FAILURE_WORDS[v] ?? v)}</option>`).join("");
    return `<label>${label}<select data-facet="${key}"><option value="">All</option>${options}</select></label>`;
  }).join("");
}

function drawTabs(): void {
  $("tabs").innerHTML = TABS.map(([id, label]) =>
    `<button type="button" role="tab" aria-selected="${id === kind}" data-kind="${id}">${label}</button>`).join("");
}

function showDrill(id: string): void {
  const drill = drills.get(id);
  if (!drill) return;
  const columns: (keyof Row)[] = kind === "decision"
    ? ["model", "case", "tier", "harmful", "answer", "policy_allows", "commit"]
    : ["model", "case", "suite", "repeat", "fill", "ending", "failure", "stage", "answer", "seconds", "commit"];
  const shown = drill.rows.slice(0, 200);
  const cell = (value: unknown) => esc(typeof value === "number" && !Number.isInteger(value) ? value.toFixed(2) : value ?? "");
  $("drill-title").textContent = `${drill.title} (${drill.rows.length} rows${drill.rows.length > 200 ? ", first 200 shown" : ""})`;
  $("drill-table").innerHTML = `<thead><tr>${columns.map((c) => `<th scope="col">${c.replace("_", " ")}</th>`).join("")}</tr></thead><tbody>${shown.map((row) => `<tr>${columns.map((c) => `<td>${cell(row[c])}</td>`).join("")}</tr>`).join("")}</tbody>`;
  $("drill").hidden = false;
  $("drill").scrollIntoView({ behavior: "smooth", block: "nearest" });
}

async function start(): Promise<void> {
  const base = $("count").dataset.base ?? "/";
  const text = await (await fetch(`${base}data/observations.jsonl`)).text();
  rows = text.replaceAll(", refusals removed", ", safety training removed").split("\n").filter(Boolean).map((line) => JSON.parse(line));
  drawTabs();
  drawFacets();
  draw();
  $("tabs").addEventListener("click", (event) => {
    const next = (event.target as HTMLElement).dataset.kind;
    if (!next) return;
    kind = next;
    for (const [key] of FACETS) delete chosen[key];
    drawTabs();
    drawFacets();
    draw();
  });
  $("facets").addEventListener("change", (event) => {
    const select = event.target as HTMLSelectElement;
    chosen[select.dataset.facet as keyof Row] = select.value;
    draw();
  });
  $("views").addEventListener("click", (event) => {
    const id = (event.target as HTMLElement).dataset.drill;
    if (id) showDrill(id);
  });
}

start().catch(() => {
  $("count").textContent = "Couldn't load the data file. Run npm run assets first.";
});
