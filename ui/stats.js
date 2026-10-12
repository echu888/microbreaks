const { invoke } = window.__TAURI__.core;
const app = document.getElementById("app");

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

// "2026-10-12T17:10:00" -> "17:10"
const hhmm = (iso) => iso.slice(11, 16);
const percent = (part, whole) => (whole ? Math.round((100 * part) / whole) + "%" : "-");

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

function render(s) {
  const day = new Date(s.date + "T00:00:00");
  const heading = day.toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric" });
  app.replaceChildren(
    el("h1", null, heading),
    breakSection("Movement breaks", s.movement),
    breakSection("Eye breaks", s.eye),
    exerciseSection(s.exercises),
    daySection(s),
  );
}

async function load() {
  render(await invoke("day_stats"));
}

// Fresh numbers every time the window comes back to the front.
window.addEventListener("focus", load);
load();
