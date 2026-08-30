// Browser history owns a compact author-facing reading position.  Entity IDs
// stay in history state, never in the URL, labels, or accessibility tree.
export function navigationSnapshot(state, { listScroll = 0 } = {}) {
  const view = state.view || "index";
  const mobilePane = view === "article" || view === "timeline" || view === "whereabouts" || view === "possibilities" || view === "chronology" ? "article" : (state.mobilePane === "nav" ? "nav" : "index");
  return {
    view,
    mobilePane,
    returnView: state.returnView || "index",
    entryId: state.selectedEntityId || "",
    timelineId: state.activeTimelineId || "",
    // Possibilities is deliberately outside the canonical reading context.
    // Do not let a horizon from the preceding view leak into a saved entry.
    horizon: view === "possibilities" ? null : (state.horizon || null),
    kind: state.kind || "",
    query: state.query || "",
    ...(Array.isArray(state.selectedThreadIds) ? { threadIds: state.selectedThreadIds.filter((value) => typeof value === "string") } : {}),
    ...(state.searchKind ? { searchKind: state.searchKind } : {}),
    ...(state.indexSort ? { indexSort: state.indexSort } : {}),
    ...(state.whereaboutsSort ? { whereaboutsSort: state.whereaboutsSort } : {}),
    ...(state.possibilitiesSort ? { possibilitiesSort: state.possibilitiesSort } : {}),
    listScroll: Number.isFinite(listScroll) ? listScroll : 0,
  };
}

export function restoreNavigation(snapshot, fallback = {}) {
  const value = snapshot && typeof snapshot === "object" ? snapshot : {};
  const horizon = value.horizon && typeof value.horizon === "object" ? value.horizon : null;
  const view = ["index", "article", "timeline", "whereabouts", "possibilities", "chronology"].includes(value.view) ? value.view : (fallback.view || "index");
  const mobilePane = view === "article" || view === "timeline" || view === "whereabouts" || view === "possibilities" || view === "chronology" ? "article" : (value.mobilePane === "nav" ? "nav" : "index");
  return {
    view,
    mobilePane,
    returnView: ["index", "timeline", "whereabouts"].includes(value.returnView) ? value.returnView : "index",
    entryId: typeof value.entryId === "string" ? value.entryId : "",
    timelineId: typeof value.timelineId === "string" ? value.timelineId : (fallback.timelineId || ""),
    // Old browser entries may predate the non-canonical view rule.  Treat
    // their horizon as inapplicable instead of restoring stale scope.
    horizon: view === "possibilities" ? null : (horizon && typeof horizon.timeline === "string" && horizon.tick != null ? horizon : null),
    kind: typeof value.kind === "string" ? value.kind : "",
    query: typeof value.query === "string" ? value.query : "",
    threadIds: Array.isArray(value.threadIds) ? value.threadIds.filter((item) => typeof item === "string") : [],
    ...(typeof value.searchKind === "string" ? { searchKind: value.searchKind } : {}),
    ...(typeof value.indexSort === "string" ? { indexSort: value.indexSort } : {}),
    ...(typeof value.whereaboutsSort === "string" ? { whereaboutsSort: value.whereaboutsSort } : {}),
    ...(typeof value.possibilitiesSort === "string" ? { possibilitiesSort: value.possibilitiesSort } : {}),
    listScroll: Number.isFinite(value.listScroll) && value.listScroll >= 0 ? value.listScroll : 0,
  };
}

export function historyAction(state, action) {
  const current = { ...state };
  if (action.type === "browse") return { ...current, view: "index", mobilePane: "index", kind: action.kind || "", query: "", selectedEntityId: "" };
  if (action.type === "open-article") return { ...current, view: "article", mobilePane: "article", selectedEntityId: action.entryId || "", returnView: action.returnView || "index" };
  if (action.type === "open-timeline") return { ...current, view: "timeline", mobilePane: "article", activeTimelineId: action.timelineId || current.activeTimelineId || "" };
  if (action.type === "open-whereabouts") return { ...current, view: "whereabouts", mobilePane: "article", selectedEntityId: "" };
  if (action.type === "open-possibilities") return { ...current, view: "possibilities", mobilePane: "article", selectedEntityId: "", horizon: null };
  if (action.type === "open-chronology") return { ...current, view: "chronology", mobilePane: "article", selectedEntityId: "" };
  if (action.type === "set-horizon") return { ...current, horizon: action.horizon || null };
  if (action.type === "back-to-list") return { ...current, view: "index", mobilePane: "index" };
  if (action.type === "open-navigation") return { ...current, mobilePane: "nav" };
  return current;
}

export function createNavigationGeneration() {
  let current = 0;
  return { begin: () => ++current, isCurrent: (generation) => generation === current };
}

// A newer navigation owns the visible reading state immediately.  Clearing
// here lets a cancelled request leave quietly without restoring stale content.
export function clearSupersededNavigationLoading({ article, articleStatus, entities, entitiesStatus }) {
  if (article) article.setAttribute("aria-busy", "false");
  if (entities) entities.setAttribute("aria-busy", "false");
  if (articleStatus) articleStatus.textContent = "";
  if (entitiesStatus && entitiesStatus.textContent === "Searching lore…") entitiesStatus.textContent = "";
}
