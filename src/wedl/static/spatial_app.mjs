import { createSpatialApi, PAGE_LIMIT, SpatialReadError } from "./spatial_api.mjs";
import { createSpatialRenderer } from "./spatial_renderer.mjs";

export const LORE_HANDOFF_KEY = "wedl.spatial.lore.once";
const node = (document, tag, value = "") => { const item = document.createElement(tag); item.textContent = String(value); return item; };
const metricText = (metric) => metric == null ? "unknown" : `${metric.value} ${metric.unit || "(unit unspecified)"}`;
const validId = (value) => typeof value === "string" && value.length > 0 && value.length <= 256 && !/[\u0000-\u001f\u007f]/.test(value);
const message = (error) => error instanceof SpatialReadError ? `${error.code ? `${error.code}: ` : ""}${error.message}` : error?.message || "The spatial read failed. Retry the action.";

export function mountSpatialExplorer(document, api = createSpatialApi(), browser = globalThis) {
  const get = (id) => document.getElementById(id);
  const el = Object.fromEntries([
    "spatial-status", "refresh-catalog", "place-search", "place-query", "place-roots", "place-back", "place-context", "place-list", "place-more",
    "selected-place", "route-outgoing", "route-incoming", "route-list", "route-more", "path-form", "path-target", "path-metric", "path-submit", "path-result",
    "map-select", "map-more", "map-native", "viewport-form", "min-x", "min-y", "max-x", "max-y", "min-z", "max-z", "map-features", "viewport-more",
    "layer-form", "story-timeline", "story-tick", "story-order", "layer-audience", "layer-perspective", "layer-list", "layer-more",
  ].map((id) => [id, get(id)]));
  const renderer = createSpatialRenderer(el["map-features"], document);
  const channels = new Map();
  const state = { maps: [], mapCursor: null, place: null, parent: null, parents: [], placeQuery: "", placeCursor: null,
    routeDirection: "outgoing", routeCursor: null, viewportCursor: null, layerCursor: null, viewportFields: null, layerFields: null,
    features: [], layers: [] };

  function status(value, error = false) { el["spatial-status"].textContent = value; el["spatial-status"].classList.toggle("error", error); }
  function stopAll() { for (const channel of channels.values()) channel.controller.abort(); channels.clear(); }
  async function read(key, action) {
    channels.get(key)?.controller.abort();
    const controller = new AbortController();
    const generation = (channels.get(key)?.generation || 0) + 1;
    channels.set(key, { controller, generation });
    try {
      const value = await action(controller.signal);
      if (channels.get(key)?.generation !== generation || controller.signal.aborted) return null;
      return value;
    } catch (error) {
      if (controller.signal.aborted || channels.get(key)?.generation !== generation) return null;
      if (error?.state === "stale_revision" || /revision|capabilities/i.test(error?.message || "") && error?.state === "error") {
        stopAll(); api.invalidate(); clearProjection();
        status("The world changed. Refresh the spatial catalog to continue.", true);
      } else status(message(error), true);
      return null;
    }
  }
  function clearProjection() {
    state.maps = []; state.mapCursor = null; state.place = null; state.parent = null; state.parents = []; state.placeQuery = ""; state.placeCursor = null; state.routeCursor = null;
    state.viewportCursor = null; state.layerCursor = null; state.features = []; state.layers = [];
    el["map-select"].replaceChildren(); el["place-list"].replaceChildren(); el["route-list"].replaceChildren();
    el["layer-list"].replaceChildren(); renderer.clear();
    el["selected-place"].textContent = "Choose a place to inspect its authored routes and lore.";
    el["path-result"].textContent = ""; el["map-native"].textContent = ""; el["place-context"].textContent = "";
    for (const id of ["map-more", "place-more", "route-more", "viewport-more", "layer-more"]) el[id].hidden = true;
    for (const id of ["route-outgoing", "route-incoming", "path-submit"]) el[id].disabled = true;
  }
  function capability(value) { return api.capabilities.includes(value); }
  function selectedMap() { return state.maps.find((map) => map.id === el["map-select"].value); }
  function mapBounds(map) {
    const minimum = map?.bounds?.min || [], maximum = map?.bounds?.max || [];
    if (minimum.length < 2 || maximum.length < 2) return;
    const threeDimensional = minimum.length === 3 && maximum.length === 3;
    for (const field of document.querySelectorAll(".z-field")) field.hidden = !threeDimensional;
    for (const id of ["min-z", "max-z"]) el[id].required = threeDimensional;
    for (const [id, value] of [["min-x", minimum[0]], ["min-y", minimum[1]], ["max-x", maximum[0]], ["max-y", maximum[1]], ["min-z", minimum[2]], ["max-z", maximum[2]]]) {
      if (Number.isFinite(value)) el[id].value = String(value);
    }
  }
  function mapInfo() {
    channels.get("viewport")?.controller.abort(); channels.get("layers")?.controller.abort();
    const map = selectedMap();
    el["map-native"].textContent = map ? `${map.label} · CRS ${map.crs} · axes ${map.axes?.join(", ")} · unit ${map.unit}` : "No authored map is available. Place browsing still works.";
    mapBounds(map); renderer.clear(); clearLayerView(); state.viewportCursor = null; el["viewport-more"].hidden = true;
  }
  function clearLayerView() { el["layer-list"].replaceChildren(); state.layers = []; state.layerCursor = null; state.layerFields = null; el["layer-more"].hidden = true; }
  function addLoreLink(container, id) {
    if (!validId(id)) return;
    const link = node(document, "a", "Read lore"); link.href = "/";
    link.addEventListener("click", (event) => {
      if (event && (event.button > 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey)) return;
      try { browser.sessionStorage?.setItem(LORE_HANDOFF_KEY, id); } catch { /* The compendium root remains a safe fallback. */ }
    });
    container.append(link);
  }
  function renderPlace(card) {
    const item = node(document, "li"); item.append(node(document, "strong", card.label || card.id));
    item.append(node(document, "p", card.geometryAvailable ? `Authored geometry on ${card.mapId || "an unknown map"}` : "No authored coordinates"));
    const inspect = node(document, "button", "Inspect"); inspect.type = "button"; inspect.addEventListener("click", () => selectPlace(card)); item.append(inspect);
    const children = node(document, "button", "Children"); children.type = "button"; children.addEventListener("click", () => {
      state.parents.push(state.parent); state.parent = card.id; state.placeQuery = ""; el["place-query"].value = ""; void loadPlaces();
    }); item.append(children); addLoreLink(item, card.id); return item;
  }
  function routeDescription(card) {
    if (card.kind === "portal") {
      const destination = card.target?.kind === "position" ? `position ${card.target.coordinates?.join(", ")} on ${card.target.mapId} (${card.target.crs}, ${card.target.unit})` : `place ${card.target?.locationId}`;
      return `Authored portal from ${card.fromLocationId} to ${destination}. Modes: ${(card.modes || []).join(", ") || "unspecified"}.`;
    }
    return `Authored ${card.authoredDirection} route ${card.fromLocationId} → ${card.toLocationId}${card.reverseOfAuthored ? " (authored two-way edge)" : ""}. ${card.availability}; ${card.uncertainty}. Route distance: ${metricText(card.routeDistance)}. Travel cost: ${metricText(card.travelCost)}. Duration: ${metricText(card.duration)}.`;
  }
  function renderRoute(card) {
    const item = node(document, "li"); item.append(node(document, "strong", card.label || card.id), node(document, "p", routeDescription(card)));
    const target = card.kind === "portal" ? (state.routeDirection === "incoming" ? card.fromLocationId : card.target?.locationId)
      : state.routeDirection === "incoming" ? card.fromLocationId : card.toLocationId;
    if (validId(target)) { const button = node(document, "button", state.routeDirection === "incoming" ? "Inspect source" : "Inspect destination"); button.type = "button"; button.addEventListener("click", () => void selectId(target)); item.append(button); }
    addLoreLink(item, card.id); return item;
  }
  function renderLayer(layer) {
    const item = node(document, "li", `${layer.label || layer.overlayId} · ${layer.locationId} · ${layer.geometry?.kind || "geometry unknown"}`);
    addLoreLink(item, layer.overlayId); return item;
  }
  async function boot() {
    stopAll(); api.invalidate(); clearProjection();
    const ready = await read("boot", (signal) => api.session(signal)); if (ready === null) return;
    await loadCatalog();
  }
  async function loadCatalog(cursor = null) {
    const result = await read("catalog", (signal) => api.catalog(signal, cursor)); if (!result) return;
    state.maps = result.maps.slice(0, PAGE_LIMIT); state.mapCursor = result.nextCursor;
    el["map-more"].hidden = !state.mapCursor;
    el["map-select"].replaceChildren(...state.maps.map((map) => { const option = node(document, "option", map.label || map.id); option.value = map.id; return option; }));
    mapInfo();
    if (!result.spatialAvailable || !capability("spatial-core-v1")) { status("This compiled world has no spatial places. The compendium remains available."); return; }
    status(`Spatial catalog ready at revision ${api.revision.slice(0, 8)}. Browse authored places.`);
    if (cursor === null) await loadPlaces();
  }
  async function loadPlaces(cursor = null) {
    if (!capability("spatial-core-v1")) return;
    const fields = state.placeQuery ? { mode: "search", query: state.placeQuery } : state.parent ? { mode: "children", parentId: state.parent } : { mode: "roots" };
    const result = await read("places", (signal) => api.explorer("places", fields, signal, cursor)); if (!result) return;
    el["place-list"].replaceChildren(...result.places.slice(0, PAGE_LIMIT).map(renderPlace));
    state.placeCursor = result.nextCursor; el["place-more"].hidden = !state.placeCursor;
    el["place-back"].disabled = !state.parents.length;
    el["place-context"].textContent = state.placeQuery ? `Search: ${state.placeQuery}` : state.parent ? `Children of ${state.parent}` : "Root places";
    if (!el["place-list"].children.length) el["place-list"].append(node(document, "li", "No places in this page."));
  }
  async function selectId(id) {
    const result = await read("selection", (signal) => api.explorer("places", { mode: "select", ids: [id] }, signal));
    if (result?.places?.length) selectPlace(result.places[0]);
    else if (result) status("The selected place is unavailable.", true);
  }
  function selectPlace(card) {
    channels.get("selection")?.controller.abort();
    state.place = card; el["selected-place"].textContent = `${card.label || card.id} · ${card.id}${card.geometryAvailable ? "" : " · no coordinates"}`;
    for (const id of ["route-outgoing", "route-incoming"]) el[id].disabled = !capability("route-v1");
    el["path-submit"].disabled = !capability("route-v1");
    el["route-list"].replaceChildren(); el["path-result"].textContent = ""; state.routeCursor = null; el["route-more"].hidden = true;
    if (capability("route-v1")) void loadRoutes("outgoing");
    else status("Directed routes are unavailable in this compiled world.");
  }
  async function loadRoutes(direction = state.routeDirection, cursor = null) {
    if (!state.place || !capability("route-v1")) return;
    if (cursor === null) { el["route-list"].replaceChildren(); state.routeCursor = null; el["route-more"].hidden = true; }
    const locationId = state.place.id; const result = await read("routes", (signal) => api.explorer("routes", { locationId, direction }, signal, cursor));
    if (!result || state.place?.id !== locationId) return;
    state.routeDirection = direction;
    el["route-list"].replaceChildren(...result.routes.slice(0, PAGE_LIMIT).map(renderRoute));
    state.routeCursor = result.nextCursor; el["route-more"].hidden = !state.routeCursor;
    if (!el["route-list"].children.length) el["route-list"].append(node(document, "li", `No authored ${direction} routes or portals.`));
    status(`Showing authored ${direction} connections for ${state.place.label || locationId}.`);
  }
  function viewportFields() {
    const map = selectedMap(); if (!map) throw new SpatialReadError("Choose an authored map first.");
    const values = ["min-x", "min-y", "max-x", "max-y"].map((id) => Number(el[id].value));
    if (["min-x", "min-y", "max-x", "max-y"].some((id) => !el[id].value) || values.some((value) => !Number.isFinite(value)) || values[0] > values[2] || values[1] > values[3]) throw new SpatialReadError("Enter finite, ordered viewport bounds.");
    const minimum = values.slice(0, 2), maximum = values.slice(2);
    if (el["min-z"].required) {
      const lower = Number(el["min-z"].value), upper = Number(el["max-z"].value);
      if (!el["min-z"].value || !el["max-z"].value || !Number.isFinite(lower) || !Number.isFinite(upper) || lower > upper) throw new SpatialReadError("Enter finite, ordered Z bounds for this map.");
      minimum.push(lower); maximum.push(upper);
    }
    return { mapId: map.id, bounds: { min: minimum, max: maximum }, relation: "intersects" };
  }
  async function loadViewport(cursor = null) {
    if (!capability("geometry-v1")) { status("This world has no authored geometry. Browse its places in the list."); return; }
    let fields;
    try { fields = cursor ? state.viewportFields : viewportFields(); } catch (error) { status(message(error), true); return; }
    const result = await read("viewport", (signal) => api.explorer("viewport", fields, signal, cursor)); if (!result) return;
    state.viewportFields = fields; state.features = result.features.slice(0, PAGE_LIMIT); state.viewportCursor = result.nextCursor;
    el["viewport-more"].hidden = !state.viewportCursor;
    renderer.render(selectedMap(), { ...result, features: state.features });
    status(`Viewport page loaded in native ${result.crs} coordinates (${result.unit}).`);
  }
  function storyTime() {
    const timeline = el["story-timeline"].value.trim(), tick = el["story-tick"].value.trim(), order = el["story-order"].value.trim();
    if (!timeline || !/^-?[0-9]+$/.test(tick) || !/^-?[0-9]+$/.test(order)) throw new SpatialReadError("Choose an exact StoryTime timeline, tick, and order before reading layers.");
    return { timeline, tick, order };
  }
  async function loadLayers(cursor = null) {
    if (cursor === null) clearLayerView();
    if (!capability("overlay-v1") || !capability("geometry-v1")) { status("Horizon layers are unavailable in this compiled world."); return; }
    let fields;
    try { fields = cursor ? state.layerFields : { ...viewportFields(), asOf: storyTime(), audience: el["layer-audience"].value.trim(), perspective: el["layer-perspective"].value.trim() }; }
    catch (error) { status(message(error), true); return; }
    if (!fields.audience || !fields.perspective) { status("Audience and perspective are required.", true); return; }
    const result = await read("layers", (signal) => api.explorer("layers", fields, signal, cursor)); if (!result) return;
    state.layerFields = fields; state.layers = result.layers.slice(0, PAGE_LIMIT); state.layerCursor = result.nextCursor;
    el["layer-more"].hidden = !state.layerCursor;
    el["layer-list"].replaceChildren(...result.layers.slice(0, PAGE_LIMIT).map(renderLayer));
    if (!el["layer-list"].children.length) el["layer-list"].append(node(document, "li", "No layers are visible for this exact horizon and lens."));
    status(`Authorized layers at ${fields.asOf.timeline}:${fields.asOf.tick}:${fields.asOf.order} for ${fields.audience}/${fields.perspective}.`);
  }
  async function loadPath() {
    if (!state.place) return;
    el["path-result"].textContent = "";
    const target = el["path-target"].value.trim(); if (!validId(target)) { status("Enter a valid destination place ID.", true); return; }
    const source = state.place.id; const metric = el["path-metric"].value;
    const result = await read("path", (signal) => api.path(source, target, metric, signal)); if (!result || state.place?.id !== source) return;
    const path = result.path || result;
    const ids = Array.isArray(path.ids) ? path.ids.join(" → ") : "No authored path";
    el["path-result"].textContent = `${ids}. ${metric}: ${path.metric?.unknown || path.metric?.computedTotal == null ? "unknown" : metricText({ value: path.metric.computedTotal, unit: path.metric.unit })}. ${path.partial ? "Partial result." : ""}`;
  }

  el["refresh-catalog"].addEventListener("click", () => { void boot(); });
  el["place-search"].addEventListener("submit", (event) => { event.preventDefault(); state.placeQuery = el["place-query"].value.trim(); state.placeCursor = null; void loadPlaces(); });
  el["place-roots"].addEventListener("click", () => { state.parent = null; state.parents = []; state.placeQuery = ""; el["place-query"].value = ""; void loadPlaces(); });
  el["place-back"].addEventListener("click", () => { state.parent = state.parents.pop() ?? null; state.placeQuery = ""; void loadPlaces(); });
  el["place-more"].addEventListener("click", () => { if (state.placeCursor) void loadPlaces(state.placeCursor); });
  for (const direction of ["incoming", "outgoing"]) el[`route-${direction}`].addEventListener("click", () => { state.routeCursor = null; void loadRoutes(direction); });
  el["route-more"].addEventListener("click", () => { if (state.routeCursor) void loadRoutes(state.routeDirection, state.routeCursor); });
  el["path-form"].addEventListener("submit", (event) => { event.preventDefault(); void loadPath(); });
  el["map-more"].addEventListener("click", () => { if (state.mapCursor) void loadCatalog(state.mapCursor); });
  el["map-select"].addEventListener("change", () => { mapInfo(); state.viewportCursor = null; state.layerCursor = null; });
  el["viewport-form"].addEventListener("submit", (event) => { event.preventDefault(); void loadViewport(); });
  el["viewport-more"].addEventListener("click", () => { if (state.viewportCursor) void loadViewport(state.viewportCursor); });
  el["layer-form"].addEventListener("submit", (event) => { event.preventDefault(); void loadLayers(); });
  for (const id of ["story-timeline", "story-tick", "story-order", "layer-audience", "layer-perspective", "min-x", "min-y", "max-x", "max-y", "min-z", "max-z"]) {
    el[id].addEventListener("input", () => { channels.get("layers")?.controller.abort(); clearLayerView(); });
  }
  el["layer-more"].addEventListener("click", () => { if (state.layerCursor) void loadLayers(state.layerCursor); });
  void boot();
  return { boot, state };
}
