// The adapter accepts only one bounded, native-CRS viewport page. It neither
// requests tiles nor projects coordinates into another map.
export function createSpatialRenderer(host, document) {
  const label = (value) => String(value ?? "unknown");
  const element = (tag, value) => { const node = document.createElement(tag); node.textContent = value; return node; };
  return {
    clear() { host.replaceChildren(); },
    render(map, projection) {
      host.replaceChildren();
      const heading = element("h3", `${label(map.label)} · ${label(projection.crs)} · ${label(projection.unit)}`);
      host.append(heading);
      const features = projection.features || [];
      if (!features.length) { host.append(element("p", "No geometry in this viewport. Browse places in the list.")); return; }
      const list = document.createElement("ul");
      for (const feature of features.slice(0, 100)) {
        const geometry = feature.geometry || {};
        list.append(element("li", `${label(feature.label)}: ${label(geometry.kind)} ${JSON.stringify(geometry.coordinates ?? [])} (${label(projection.crs)}, ${label(projection.unit)})`));
      }
      host.append(list);
    },
  };
}
