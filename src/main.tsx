import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";
import { clampSize, restorePreferences, preset, DEFAULTS, STORAGE_KEY, reconcileTabs, openTab, closeTab, cycleTab } from "./workspace";
import type { Preferences, Tabs, Panel } from "./workspace";
import type { PointerEvent as ReactPointerEvent } from "react";

type Node = { id: string; name?: string; element: { type: string; kind?: string }; children?: Node[]; properties?: Record<string, unknown> };
type Screen = { id: string; name: string; root: Node };
type Snapshot = { revision: number; project: { metadata: { name: string }; screens: Screen[] }; diagnostics: { severity: string; code: string; path: string; message: string }[]; status: { dirty: boolean; canUndo: boolean; canRedo: boolean; undoLabel?: string; redoLabel?: string; errorCount: number; warningCount: number } };
type Command =
  | { op: "insertNode"; parent: string; node: Node; assignNewIds: true }
  | { op: "removeNode"; node: string }
  | { op: "createScreen"; name: string }
  | { op: "renameScreen"; screen: string; name: string }
  | { op: "setProperty"; node: string; name: string; value: string };
type Request = { request: "snapshot" | "validate" } | { request: "undo" | "redo"; expectedRevision: number } | { request: "execute"; transaction: { commands: Command[]; expectedRevision: number; label: string } };
type Reply = { response: { response: string; error?: { message: string }; outcome?: { allocatedIds: Record<string,string> } }; snapshot: Snapshot };
function readPanels(): Preferences {
  try { return restorePreferences(localStorage.getItem(STORAGE_KEY),localStorage.getItem("studio.workspace.v1")); }
  catch { return {...DEFAULTS}; }
}
type SizeKey = "explorerSize" | "inspectorSize" | "diagnosticsSize";
function Splitter({label,size,vertical,direction,change}:{label:string;size:number;vertical:boolean;direction:1|-1;change:(n:number)=>void}) {
  const drag=useRef<{origin:number;size:number}|null>(null);
  function position(e:ReactPointerEvent<HTMLDivElement>){return vertical?e.clientX:e.clientY;}
  return <div className={"splitter "+(vertical?"vertical":"horizontal")} role="separator" tabIndex={0} aria-label={label} aria-orientation={vertical?"vertical":"horizontal"} aria-valuemin={1} aria-valuemax={6} aria-valuenow={size}
    onPointerDown={e=>{if(e.button!==0)return;drag.current={origin:position(e),size};e.currentTarget.setPointerCapture(e.pointerId);e.preventDefault();}}
    onPointerMove={e=>{if(drag.current)change(clampSize(drag.current.size+direction*(position(e)-drag.current.origin)/(vertical?35:24)));}}
    onLostPointerCapture={()=>{drag.current=null;}} onPointerUp={()=>{drag.current=null;}} onPointerCancel={()=>{drag.current=null;}}
    onKeyDown={e=>{const step=vertical?(e.key==="ArrowRight"?1:e.key==="ArrowLeft"?-1:0):(e.key==="ArrowDown"?1:e.key==="ArrowUp"?-1:0);if(step){e.preventDefault();change(clampSize(size+direction*step));}else if(e.key==="Home"||e.key==="End"){e.preventDefault();change(e.key==="Home"?1:6);}}}/>;
}
function findNode(n: Node, id: string | null): Node | undefined {
  if(n.id === id) return n;
  for(const child of n.children ?? []) { const found=findNode(child,id); if(found) return found; }
}
function label(n: Node): string { return n.name ?? (typeof n.properties?.text === "string" ? n.properties.text : n.element.kind ?? n.element.type); }
function Tree({node,selected,select,depth=0}:{node:Node;selected:string|null;select:(id:string)=>void;depth?:number}) {
  return <><button className={"layer depth-"+Math.min(depth,5)+(selected===node.id?" active":"")} onClick={()=>select(node.id)}>{node.children?.length?"▾":"·"} {label(node)}</button>{node.children?.map(n=><Tree key={n.id} node={n} selected={selected} select={select} depth={depth+1}/>)}</>;
}
function Preview({node,selected,select}:{node:Node;selected:string|null;select:(id:string)=>void}) {
  const kind=node.element.kind ?? node.element.type;
  if(node.element.type!=="component") return <div className={"layout-preview "+(kind==="row"?"row-preview":"column-preview")}><button className={"container-label "+(selected===node.id?" active":"")} onClick={()=>select(node.id)}>{label(node)}</button>{node.children?.map(n=><Preview key={n.id} node={n} selected={selected} select={select}/>)}</div>;
  return <button className={"object "+kind+(selected===node.id?" selected":"")} onClick={()=>select(node.id)}>{kind==="gauge"?<><small>Gauge · static preview</small><strong>{String(node.properties?.value ?? 0)}</strong></>:label(node)}</button>;
}
// Explicit commits avoid starting an IPC edit during the blur that precedes
// a toolbar/menu click. Rejected edits must not masquerade as saved values.
function EditField({label,value,disabled,normalize,commit}:{label:string;value:string;disabled:boolean;normalize?:(value:string)=>string;commit:(value:string)=>void}) {
  const [draft,setDraft]=useState(value);
  const next=normalize?normalize(draft):draft;
  return <form className="field" onSubmit={e=>{e.preventDefault();if(disabled||next===value||(normalize!==undefined&&!next))return;setDraft(value);commit(next);}}>
    <label>{label}<input value={draft} disabled={disabled} onChange={e=>setDraft(e.currentTarget.value)} onKeyDown={e=>{if(e.key==="Escape"){e.stopPropagation();setDraft(value);}}}/></label>
    <button type="submit" disabled={disabled||next===value||(normalize!==undefined&&!next)}>Apply</button>
  </form>;
}
function Studio(){
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null);
  const [status,setStatus]=useState("Connecting to engineering core…");
  const [menu,setMenu]=useState<string|null>(null);
  const [busy,setBusy]=useState(false);
  const [tabs,setTabs]=useState<Tabs>({open:[],active:null});
  const [selected,setSelected]=useState<string|null>(null);
  const [panels,setPanels]=useState<Preferences>(readPanels);
  const [tab,setTab]=useState<"properties"|"bindings"|"events">("properties");
  const inFlight=useRef(false);
  const screen=snapshot?.project.screens.find(s=>s.id===tabs.active);
  const chosen=screen?findNode(screen.root,selected):undefined;
  const send=useCallback(async(request:Request)=>{
    if(inFlight.current){setStatus("Another engineering command is pending; retry after it finishes.");return;}
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<Reply>("engineering_request",{request});
      setSnapshot(reply.snapshot);
      setTabs(t=>{const ids=reply.snapshot.project.screens.map(s=>s.id);const next=reconcileTabs(t,ids);return request.request==="snapshot"&&ids.length&&!next.open.length?openTab(next,ids[0]):next;});
      if(reply.response.response==="error")setStatus(reply.response.error?.message??"Command rejected");
      else {setStatus("Engineering core ready · revision "+reply.snapshot.revision);const ids=reply.response.outcome?.allocatedIds;if(ids?.["0:screen"]){setTabs(t=>openTab(t,ids["0:screen"]));setSelected(null);}if(ids?.["0:node:pending"])setSelected(ids["0:node:pending"]);}
    }catch(e){setStatus("Command failed: "+String(e));}
    finally{inFlight.current=false;setBusy(false);}
  },[]);
  useEffect(()=>{void send({request:"snapshot"});},[send]);
  useEffect(()=>{try{localStorage.setItem(STORAGE_KEY,JSON.stringify(panels));}catch{/* only presentation preferences */}},[panels]);
  const execute=(command:Command,label:string)=>{if(snapshot)void send({request:"execute",transaction:{commands:[command],expectedRevision:snapshot.revision,label}})};
  const history=useCallback((request:"undo"|"redo")=>{if(snapshot && (request==="undo"?snapshot.status.canUndo:snapshot.status.canRedo))void send({request,expectedRevision:snapshot.revision});},[snapshot,send]);
  const toggle=(key:Panel)=>setPanels(p=>({...p,[key]:!p[key]}));
  const resize=(key:SizeKey,n:number)=>setPanels(p=>({...p,[key]:clampSize(n)}));
  const activate=(id:string)=>{if(!inFlight.current){setTabs(t=>openTab(t,id));setSelected(null);}};
  const close=(id:string)=>{if(!inFlight.current){setTabs(t=>closeTab(t,id));if(tabs.active===id)setSelected(null);}};
  const cycle=(delta:number)=>{if(!inFlight.current){setTabs(t=>cycleTab(t,delta));setSelected(null);}};
  const insert=(kind:"button"|"text"|"gauge")=>{if(screen)execute({op:"insertNode",parent:screen.root.id,node:{id:"pending",element:{type:"component",kind},properties:kind==="gauge"?{value:0,min:0,max:100}:{text:kind==="button"?"Button":"Text"}},assignNewIds:true},"Insert "+kind);};
  type Action = {run:()=>void; enabled:boolean; shortcut?:string; checked?:boolean};
  const ready=!!snapshot&&!busy;
  const actions:Record<string,Action>={
    "New screen":{run:()=>execute({op:"createScreen",name:"Screen "+((snapshot?.project.screens.length??0)+1)},"New screen"),enabled:ready},
    "Undo":{run:()=>history("undo"),enabled:ready&&!!snapshot?.status.canUndo,shortcut:"Mod+Z"},
    "Redo":{run:()=>history("redo"),enabled:ready&&!!snapshot?.status.canRedo,shortcut:"Mod+Shift+Z"},
    "Delete selection":{run:()=>{if(chosen)execute({op:"removeNode",node:chosen.id},"Delete selection");},enabled:ready&&!!chosen&&chosen.id!==screen?.root.id,shortcut:"Delete"},
    "Button":{run:()=>insert("button"),enabled:ready&&!!screen},
    "Text":{run:()=>insert("text"),enabled:ready&&!!screen},
    "Gauge":{run:()=>insert("gauge"),enabled:ready&&!!screen},
    "Validate project":{run:()=>void send({request:"validate"}),enabled:ready},
    "Project explorer":{run:()=>toggle("left"),enabled:!busy,checked:panels.left},
    "Inspector":{run:()=>toggle("right"),enabled:!busy,checked:panels.right},
    "Diagnostics":{run:()=>toggle("diagnostics"),enabled:!busy,checked:panels.diagnostics},
    "Swap side docks":{run:()=>setPanels(p=>({...p,swapped:!p.swapped})),enabled:!busy},
    "Standard layout":{run:()=>setPanels(preset("standard")),enabled:!busy},
    "Design layout":{run:()=>setPanels(preset("design")),enabled:!busy},
    "Diagnostics layout":{run:()=>setPanels(preset("diagnostics")),enabled:!busy},
    "Reset workspace":{run:()=>setPanels({...DEFAULTS}),enabled:!busy},
    "Close tab":{run:()=>{if(tabs.active)close(tabs.active);},enabled:ready&&!!tabs.active,shortcut:"Mod+W"},
    "Next tab":{run:()=>cycle(1),enabled:ready&&tabs.open.length>1,shortcut:"Mod+Tab"},
    "Previous tab":{run:()=>cycle(-1),enabled:ready&&tabs.open.length>1,shortcut:"Mod+Shift+Tab"},
    "Exit (discard session)":{run:()=>{if(window.confirm("Exit and discard this in-memory session? Project saving is not available in this slice."))void invoke("discard_and_exit").catch(e=>setStatus(String(e)));},enabled:ready}
  };
  function action(name:string){setMenu(null);if(!inFlight.current&&actions[name]?.enabled)actions[name].run();}
  const disabled=(name:string)=>!actions[name]?.enabled;
  useEffect(()=>{
    function keys(e:KeyboardEvent){
      if(e.key==="Escape"){setMenu(null);return;}
      if(e.defaultPrevented||e.isComposing||e.altKey||e.target instanceof HTMLInputElement||e.target instanceof HTMLTextAreaElement||e.target instanceof HTMLSelectElement||(e.target instanceof HTMLElement&&e.target.isContentEditable))return;
      const key=(e.ctrlKey||e.metaKey?"Mod+":"")+(e.shiftKey?"Shift+":"")+(e.key.length===1?e.key.toUpperCase():e.key);
      const name=key==="Mod+Y"?"Redo":Object.keys(actions).find(n=>actions[n].shortcut===key);
      if(name){e.preventDefault();if(!e.repeat)action(name);}
    }
    window.addEventListener("keydown",keys);return()=>window.removeEventListener("keydown",keys);
  });
  const menus:Record<string,string[]>={File:["New screen","Open project (planned)","Save project (planned)","Close tab","Exit (discard session)"],Edit:["Undo","Redo","Delete selection"],View:["Project explorer","Inspector","Diagnostics","Swap side docks","Standard layout","Design layout","Diagnostics layout","Reset workspace","Next tab","Previous tab"],Insert:["Button","Text","Gauge"],Arrange:["Align (planned)"],Project:["Validate project"],PLC:["Connections (planned)"],Simulation:["Preview (planned)"],Runtime:["Export (planned)","Publish (planned)"],Tools:["Preferences (planned)"],Help:["About (planned)"]};
  return <main className="studio" onClick={()=>setMenu(null)}>
    <header><strong>◈ HMI STUDIO</strong><span>{snapshot?.project.metadata.name??"Loading"}{snapshot?.status.dirty?" *":""} · Untitled session</span><small>RUST-02B</small></header>
    <nav aria-label="Main menu" onClick={e=>e.stopPropagation()}>{Object.entries(menus).map(([name,items])=><div className="menu" key={name}><button aria-expanded={menu===name} onClick={()=>setMenu(menu===name?null:name)}>{name}</button>{menu===name&&<div className="dropdown">{items.map(item=><button disabled={disabled(item)} key={item} onClick={()=>action(item)}>{item}{actions[item]?.checked?" ✓":""}{actions[item]?.shortcut&&<small className="shortcut">{actions[item].shortcut}</small>}</button>)}</div>}</div>)}</nav>
    <div className="toolbar">{["New screen","Undo","Redo","Button","Text","Gauge","Delete selection","Validate project"].map(name=><button key={name} disabled={disabled(name)} onClick={()=>action(name)}>{name}</button>)}<span className="grow"/>{["Project explorer","Inspector","Diagnostics"].map(name=><button key={name} disabled={disabled(name)} aria-pressed={actions[name].checked} onClick={()=>action(name)}>{name}</button>)}</div>
    <section className={"workspace dock-workspace "+(panels.swapped?"swapped":"")}>
    {panels.left&&<aside className={"left dock-panel size-"+panels.explorerSize}><h3>PROJECT EXPLORER</h3><div className="tree">{snapshot?.project.screens.map(s=><button key={s.id} className={"layer "+(screen?.id===s.id?"active":"")} disabled={busy} onClick={()=>activate(s.id)}>{s.name}</button>)}</div><h3>COMPONENTS / LAYERS</h3>{screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</aside>}
    {panels.left&&<div className="explorer-split"><Splitter label="Resize explorer" size={panels.explorerSize} vertical direction={panels.swapped?-1:1} change={n=>resize("explorerSize",n)}/></div>}
    <section className="designer"><div className="screen-tabs" aria-label="Open screens">{tabs.open.map(id=>{const s=snapshot?.project.screens.find(x=>x.id===id);return s&&<div className="screen-tab" key={id}><button aria-current={tabs.active===id?"page":undefined} className={tabs.active===id?"active":""} disabled={busy} onClick={()=>activate(id)}>{s.name}</button><button aria-label={"Close "+s.name} disabled={busy} onClick={()=>close(id)}>×</button></div>;})}</div><div className="viewport">{!screen&&<p className="muted">Open a screen in Project Explorer or create a new screen.</p>}{screen&&<div className="screen"><div className="screen-title">{screen.name} · structural preview</div><Preview node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/></div>}</div></section>
    {panels.right&&<div className="inspector-split"><Splitter label="Resize inspector" size={panels.inspectorSize} vertical direction={panels.swapped?1:-1} change={n=>resize("inspectorSize",n)}/></div>}
    {panels.right&&<aside className={"right dock-panel size-"+panels.inspectorSize}><h3>INSPECTOR</h3><div className="inspector-tabs">{(["properties","bindings","events"] as const).map(t=><button key={t} className={tab===t?"active":""} onClick={()=>setTab(t)}>{t}</button>)}</div>{tab==="properties"?<>{screen&&<EditField key={screen.id+":"+snapshot?.revision} label="Screen name" value={screen.name} disabled={busy} normalize={v=>v.trim()} commit={name=>execute({op:"renameScreen",screen:screen.id,name},"Rename screen")}/>}{chosen?<><dl><dt>ID</dt><dd>{chosen.id}</dd><dt>Type</dt><dd>{chosen.element.kind??chosen.element.type}</dd></dl>{(chosen.element.kind==="text"||chosen.element.kind==="button")&&<EditField key={chosen.id+":"+snapshot?.revision} label="Text" value={typeof chosen.properties?.text==="string"?chosen.properties.text:""} disabled={busy||typeof chosen.properties?.text==="object"} commit={value=>execute({op:"setProperty",node:chosen.id,name:"text",value},"Edit text")}/>}</>:<p>Select a layer.</p>}</>:<p>Editing {tab} is planned.</p>}</aside>}
    </section>
    {panels.diagnostics&&<Splitter label="Resize diagnostics" size={panels.diagnosticsSize} vertical={false} direction={-1} change={n=>resize("diagnosticsSize",n)}/>}
    {panels.diagnostics&&<section className={"diagnostics dock-diagnostics size-"+panels.diagnosticsSize}><h3>DIAGNOSTICS <span>{snapshot?.status.errorCount??0} errors · {snapshot?.status.warningCount??0} warnings</span></h3><p role="status">{status}</p>{snapshot?.diagnostics.map((d,i)=><p key={i}>{d.severity} · {d.code} · {d.path} · {d.message}</p>)}</section>}
    <footer><span role="status">{status}</span><span>{snapshot?.status.dirty?"Unsaved · File > Exit (discard session)":"In-memory project"} · No live PLC</span></footer>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Studio/>);
