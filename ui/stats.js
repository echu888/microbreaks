const { invoke } = window.__TAURI__.core;
const app = document.getElementById("app");
let view = "today"; // "today" | "week"

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// "2026-10-12T17:10:00" -> "17:10"
const hhmm = (iso) => iso.slice(11, 16);
const percent = (part, whole) => (whole ? Math.round((100 * part) / whole) + "%" : "-");
const sum = (exercises, key) => exercises.reduce((n, e) => n + e[key], 0);
const dayLabel = (date, options) => new Date(date + "T00:00:00").toLocaleDateString(undefined, options);

function tabs() {
  const bar = el("div", "tabs");
  for (const [id, label] of [["today", "Today"], ["week", "Last 7 days"]]) {
    const tab = el("button", id === view ? "tab active" : "tab", label);
    tab.addEventListener("click", () => {
      view = id;
      load();
    });
    bar.append(tab);
  }
  return bar;
}

function breakSection(title, k) {
  const section = el("div");
  section.append(el("h2", null, title));
  section.append(el("div", "big", `${k.done} of ${k.due} done`));
  const counts = el("div", "counts");
  for (const [label, n] of [["skipped", k.skipped], ["missed", k.missed], ["paused", k.paused]]) {
    counts.append(el("span", null, `${n} ${label}`));
  }
  section.append(counts);
  return section;
}

function exerciseSection(exercises) {
  const section = el("div");
  section.append(el("h2", null, "Reps"));
  if (exercises.length === 0) {
    section.append(el("div", "muted", "No reps logged yet."));
    return section;
  }
  for (const e of exercises) {
    const line = el("div", "line");
    line.append(el("span", null, e.name), el("span", null, `${e.reps} / ${e.target} (${percent(e.reps, e.target)})`));
    section.append(line);
  }
  return section;
}

function daySection(s) {
  const section = el("div");
  section.append(el("h2", null, "Day"));
  const end = el("div", "line");
  end.append(el("span", null, "Work day"), el("span", null, s.day_end ? `ended ${hhmm(s.day_end)}` : "still going"));
  section.append(end);
  if (s.sleep) {
    const bed = el("div", "line");
    bed.append(el("span", null, "Bedtime"), el("span", null, hhmm(s.sleep)));
    section.append(bed);
  }
  return section;
}

function renderToday(s) {
  const heading = dayLabel(s.date, { weekday: "long", month: "short", day: "numeric" });
  app.replaceChildren(
    tabs(),
    el("h1", null, heading),
    breakSection("Movement breaks", s.movement),
    breakSection("Eye breaks", s.eye),
    exerciseSection(s.exercises),
    daySection(s),
  );
}

const ratio = (k) => (k.due ? `${k.done}/${k.due}` : "-");

function weekRow(d) {
  const row = el("div", "week-row");
  row.append(el("span", null, dayLabel(d.date, { weekday: "short", month: "short", day: "numeric" })));
  if (!d.has_data) {
    row.classList.add("empty");
    row.append(el("span", "span-all", "no data"));
    return row;
  }
  const reps = percent(sum(d.exercises, "reps"), sum(d.exercises, "target"));
  row.append(
    el("span", null, ratio(d.movement)),
    el("span", null, ratio(d.eye)),
    el("span", null, d.exercises.length ? reps : "-"),
    el("span", null, d.day_end ? hhmm(d.day_end) : "-"),
  );
  return row;
}

function weekSummaryLine(label, text) {
  const line = el("div", "line");
  line.append(el("span", null, label), el("span", null, text));
  return line;
}

function renderWeek(w) {
  const header = el("div", "week-row week-head");
  for (const label of ["Day", "Move", "Eye", "Reps", "Ended"]) header.append(el("span", null, label));

  const summary = el("div");
  summary.append(el("h2", null, "Week"));
  const kindText = (k) => `${k.done} of ${k.due} done (${percent(k.done, k.due)})`;
  summary.append(weekSummaryLine("Movement breaks", kindText(w.movement)));
  summary.append(weekSummaryLine("Eye breaks", kindText(w.eye)));
  const reps = sum(w.exercises, "reps");
  const target = sum(w.exercises, "target");
  summary.append(weekSummaryLine("Reps", target ? `${reps} / ${target} (${percent(reps, target)})` : "-"));

  app.replaceChildren(tabs(), el("h1", null, "Last 7 days"), header, ...w.days.map(weekRow), summary);
}

async function load() {
  if (view === "week") renderWeek(await invoke("week_stats"));
  else renderToday(await invoke("day_stats"));
}

// Fresh numbers every time the window comes back to the front.
window.addEventListener("focus", load);
load();
