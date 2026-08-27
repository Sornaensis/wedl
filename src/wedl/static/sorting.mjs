// Pure presentation ordering helpers.  They never mutate API-owned arrays.
export const PROMINENCE_SORT_HIGH = "prominence-high";
export const PROMINENCE_SORT_LOW = "prominence-low";
export const NAME_ASC = "name-asc";
export const NAME_DESC = "name-desc";

const indexOptions = (values) => values.map(([value, label]) => ({ value, label }));
export function indexSortConfiguration({ query = "", kind = "" } = {}) {
  if (query) {
    return { defaultValue: "relevance", options: indexOptions([["relevance", "Relevance"], [NAME_ASC, "Name: A to Z"], [NAME_DESC, "Name: Z to A"], ...(kind === "character" ? [[PROMINENCE_SORT_HIGH, "Prominence: high to low"], [PROMINENCE_SORT_LOW, "Prominence: low to high"]] : [])]) };
  }
  if (kind === "character") return { defaultValue: PROMINENCE_SORT_HIGH, options: indexOptions([[PROMINENCE_SORT_HIGH, "Prominence: high to low"], [PROMINENCE_SORT_LOW, "Prominence: low to high"], [NAME_ASC, "Name: A to Z"], [NAME_DESC, "Name: Z to A"]]) };
  if (!kind) return { defaultValue: "kind-name", options: indexOptions([["kind-name", "Kind, then name"], [NAME_ASC, "Name: A to Z"], [NAME_DESC, "Name: Z to A"]]) };
  return { defaultValue: NAME_ASC, options: indexOptions([[NAME_ASC, "Name: A to Z"], [NAME_DESC, "Name: Z to A"]]) };
}
export const whereaboutsSortConfiguration = () => ({ defaultValue: PROMINENCE_SORT_HIGH, options: indexOptions([[PROMINENCE_SORT_HIGH, "Prominence: high to low"], [PROMINENCE_SORT_LOW, "Prominence: low to high"], [NAME_ASC, "Name: A to Z"], ["place-asc", "Recorded place: A to Z"], ["presence", "Presence"]]) });
export const possibilitiesSortConfiguration = () => ({ defaultValue: "status-name", options: indexOptions([["status-name", "Status, then name"], [NAME_ASC, "Name: A to Z"], [NAME_DESC, "Name: Z to A"]]) });

export function prominenceBand(score) {
  if (!Number.isFinite(score)) return "unavailable";
  if (score === 0) return "none";
  if (score < 33.33) return "lower";
  if (score < 66.67) return "moderate";
  return "higher";
}
function record(item) { return item && item.entity ? item.entity : (item && item.character ? item.character : item) || {}; }
function title(item) { return String(record(item).title || ""); }
function id(item) { return String(record(item).id || ""); }
export function compareTitle(first, second) {
  const a = title(first); const b = title(second);
  return a.localeCompare(b, undefined, { sensitivity: "accent" }) || a.localeCompare(b) || id(first).localeCompare(id(second));
}
function score(item, importanceByCharacter) { const value = importanceByCharacter && importanceByCharacter.get(id(item)); return Number.isFinite(value) ? value : null; }
export function compareCharacters(first, second, direction = "high", importanceByCharacter = new Map()) {
  const a = score(first, importanceByCharacter); const b = score(second, importanceByCharacter);
  if (a != null && b == null) return -1;
  if (a == null && b != null) return 1;
  if (a != null && b != null && a !== b) return direction === "low" ? a - b : b - a;
  return compareTitle(first, second);
}
export function sortIndex(items, value, importanceByCharacter = new Map()) {
  const copy = [...items];
  if (value === "relevance") return copy;
  return copy.sort((a, b) => value === PROMINENCE_SORT_HIGH ? compareCharacters(a, b, "high", importanceByCharacter) : value === PROMINENCE_SORT_LOW ? compareCharacters(a, b, "low", importanceByCharacter) : value === "kind-name" ? (String(record(a).kind || "").localeCompare(String(record(b).kind || "")) || compareTitle(a, b)) : (value === NAME_DESC ? -compareTitle(a, b) : compareTitle(a, b)));
}
function recordedPlace(item) { const place = item.location || item.lastKnownLocation; return place ? String(place.title || place.id || "") : "\uffff"; }
const presenceRank = { "active-scene": 0, offstage: 1, unlocated: 2 };
export function sortWhereabouts(items, value, importanceByCharacter = new Map()) {
  const copy = [...items];
  return copy.sort((a, b) => {
    if (value === PROMINENCE_SORT_HIGH || value === PROMINENCE_SORT_LOW) return compareCharacters(a, b, value === PROMINENCE_SORT_LOW ? "low" : "high", importanceByCharacter);
    if (value === "place-asc") return recordedPlace(a).localeCompare(recordedPlace(b), undefined, { sensitivity: "accent" }) || compareTitle(a, b);
    if (value === "presence") return (presenceRank[a.presence] ?? 3) - (presenceRank[b.presence] ?? 3) || compareTitle(a, b);
    return compareTitle(a, b);
  });
}
const statusRank = { open: 0, adopted: 1, rejected: 2 };
export function sortPossibilities(items, value) {
  return [...items].sort((a, b) => value === "status-name" ? ((statusRank[a.status] ?? 3) - (statusRank[b.status] ?? 3) || compareTitle(a, b)) : (value === NAME_DESC ? -compareTitle(a, b) : compareTitle(a, b)));
}
export function validSort(value, configuration) { return configuration.options.some((option) => option.value === value) ? value : configuration.defaultValue; }
