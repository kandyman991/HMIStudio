// Presentation state only; no project mutations or native capabilities.
export type Panel = "left" | "right" | "diagnostics";
export type Preferences = { version: 2; left: boolean; right: boolean; diagnostics: boolean; swapped: boolean; explorerSize: number; inspectorSize: number; diagnosticsSize: number };
export type Preset = "standard" | "design" | "diagnostics";
export const STORAGE_KEY = "studio.workspace.v2";
export const DEFAULTS: Preferences = {version:2,left:true,right:true,diagnostics:true,swapped:false,explorerSize:4,inspectorSize:5,diagnosticsSize:6};
export function clampSize(value: number): number {
  return Number.isFinite(value) ? Math.max(1,Math.min(6,Math.round(value))) : 4;
}
export function restorePreferences(raw: string | null, legacy: string | null): Preferences {
  try {
    if (raw !== null) {
      const p = JSON.parse(raw);
      if (!p || p.version !== 2) return {...DEFAULTS};
      const result = {...DEFAULTS};
      for (const k of ["left","right","diagnostics","swapped"] as const) if (typeof p[k] === "boolean") result[k]=p[k];
      for (const k of ["explorerSize","inspectorSize","diagnosticsSize"] as const) if (typeof p[k] === "number" && Number.isFinite(p[k])) result[k]=clampSize(p[k]);
      return result;
    }
    const p = JSON.parse(legacy ?? "null");
    if(p && ["left","right","diagnostics"].every(k=>typeof p[k]==="boolean")) return {...DEFAULTS,left:p.left,right:p.right,diagnostics:p.diagnostics};
  } catch { /* malformed preferences are not project data */ }
  return {...DEFAULTS};
}
export function preset(name: Preset): Preferences {
  return {...DEFAULTS,...(name==="design"?{left:false,right:true,diagnostics:false}:name==="diagnostics"?{diagnosticsSize:6,inspectorSize:2}:{})};
}
export type Tabs = {open: string[]; active: string | null};
export function reconcileTabs(t: Tabs, ids: string[]): Tabs {
  const valid = new Set(ids);
  const open = [...new Set(t.open.filter(id=>valid.has(id)))];
  return {open,active:t.active && open.includes(t.active)?t.active:open[0]??null};
}
export function openTab(t: Tabs, id: string): Tabs {
  return {open:t.open.includes(id)?t.open:[...t.open,id],active:id};
}
export function closeTab(t: Tabs, id: string): Tabs {
  const index=t.open.indexOf(id);
  if(index<0)return t;
  const open=t.open.filter(x=>x!==id);
  return {open,active:t.active===id?(open[Math.min(index,open.length-1)]??null):t.active};
}
export function cycleTab(t: Tabs, delta: number): Tabs {
  if(!t.open.length)return t;
  const index=t.active===null?-1:t.open.indexOf(t.active);
  return {...t,active:t.open[(index+delta+t.open.length)%t.open.length]};
}

// Filter only the presentation catalog; never alter project data.
export function matchesCatalog(label:string,query:string):boolean {
  return label.toLowerCase().includes(query.trim().toLowerCase());
}
