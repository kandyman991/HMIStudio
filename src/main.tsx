import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";
import { clampSize, restorePreferences, preset, DEFAULTS, STORAGE_KEY, reconcileTabs, openTab, closeTab, cycleTab } from "./workspace";
import type { Preferences, Tabs, Panel } from "./workspace";
import type { PointerEvent as ReactPointerEvent } from "react";
import { insertionParent, siblingDestination } from "./designer";
import { nodeStyle, previewLimit, isFlow, parseFixed, fitPreview, changeZoom, PREVIEW_WIDTH, PREVIEW_HEIGHT } from "./layout-preview";
import type { Element, Layout, Size, Arrangement, Cross } from "./layout-preview";

type Node = { id: string; name?: string; element: Element; layout?: Layout; responsive?: unknown[]; bindings?: unknown[]; style?: string; events?: unknown[]; children?: Node[]; properties?: Record<string, unknown> };
type Screen = { id: string; name: string; root: Node };
type Snapshot = { revision: number; project: { metadata: { name: string }; screens: Screen[] }; diagnostics: { severity: string; code: string; path: string; message: string }[]; status: { dirty: boolean; canUndo: boolean; canRedo: boolean; undoLabel?: string; redoLabel?: string; errorCount: number; warningCount: number; path?: string | null; autosaveCurrent: boolean } };
type Command =
  | { op: "insertNode"; parent: string; node: Node; assignNewIds: true }
  | { op: "removeNode"; node: string }
  | { op: "reorderNode"; node: string; index: number }
  | { op: "setLayout"; node: string; layout: Layout }
  | { op: "setElement"; node: string; element: Element }
  | { op: "createScreen"; name: string }
  | { op: "renameScreen"; screen: string; name: string }
  | { op: "setProperty"; node: string; name: string; value: string };
type Request = { request: "snapshot" | "validate" } | { request: "undo" | "redo"; expectedRevision: number } | { request: "execute"; transaction: { commands: Command[]; expectedRevision: number; label: string } };
type Reply = { generation: number; response: { response: string; error?: { message: string }; outcome?: { allocatedIds: Record<string,string> } }; snapshot: Snapshot };
type ProjectAction = "open" | "save" | "saveAs" | "recover" | "resolveUntitled" | "exit" | "newProject";
type ProjectReply = { snapshot: Snapshot; generation: number; replaced: boolean; message: string; pendingUntitled: boolean };
type PersistenceStatus = { pendingUntitled: boolean; autosaveMessage: string };
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
function Preview({node,selected,select,parent}:{node:Node;selected:string|null;select:(id:string)=>void;parent?:Element}) {
  const kind=node.element.kind??node.element.type, limits=previewLimit(node);
  const style=nodeStyle(node,parent);
  // Selection chrome is an outline, never a participating child of the flow.
  const cls="preview-node"+(selected===node.id?" preview-selected":"");
  if(isFlow(node.element))return <div className={cls+" preview-flow"} style={style} title={label(node)+(limits.length?" · "+limits.join("; "):"")} onClick={e=>{e.stopPropagation();select(node.id);}}>{node.children?.map(n=><Preview key={n.id} node={n} parent={node.element} selected={selected} select={select}/>)}</div>;
  if(node.element.type!=="component")return <button type="button" className={cls+" preview-unsupported"} style={style} onClick={e=>{e.stopPropagation();select(node.id);}} title={limits.join("; ")}>Unsupported · {label(node)} (see layers)</button>;
  return <button type="button" className={cls+" preview-component "+(["text","button","gauge"].includes(kind)?kind:"placeholder")} style={style} title={limits.join("; ")||label(node)} onClick={e=>{e.stopPropagation();select(node.id);}}>{kind==="gauge"?<><small>Static gauge</small><strong>{String(node.properties?.value??0)}</strong></>:kind==="text"||kind==="button"?(typeof node.properties?.text==="string"?node.properties.text:node.properties?.text!==undefined?"[Unresolved text]":label(node)):"Placeholder · "+kind}</button>;
}
function countLimits(node:Node):number{return previewLimit(node).length+(node.children??[]).reduce((sum,n)=>sum+countLimits(n),0);}
function SizeEditor({axis,size,disabled,commit}:{axis:string;size:Size|undefined;disabled:boolean;commit:(s:Size)=>void}){
  const current=size??{mode:"auto"} as Size;
  const [mode,setMode]=useState<Size["mode"]>(current.mode);
  const [value,setValue]=useState(current.mode==="fixed"?String(current.value):current.mode==="fill"?String((current.fraction??1)*100):"100");
  const n=parseFixed(value);
  const valid=mode==="auto"||(n!==undefined&&(mode==="fixed"||(n>0&&n<=100)));
  return <form className="layout-field" onSubmit={e=>{e.preventDefault();if(disabled||!valid)return;const next:Size=mode==="auto"?{mode}:mode==="fixed"?{mode,value:n!}:{mode,fraction:n!/100};setMode(current.mode);setValue(current.mode==="fixed"?String(current.value):current.mode==="fill"?String((current.fraction??1)*100):"100");commit(next);}}>
    <label>{axis}<select disabled={disabled} value={mode} onChange={e=>setMode(e.currentTarget.value as Size["mode"])}><option value="auto">Hug / auto</option><option value="fixed">Fixed dp</option><option value="fill">Fill %</option></select></label>
    {mode!=="auto"&&<input aria-label={axis+(mode==="fixed"?" dp":" percent")} type="number" min={0} max={mode==="fill"?100:undefined} step="any" value={value} disabled={disabled} onChange={e=>setValue(e.currentTarget.value)}/>}
    <button type="submit" disabled={disabled||!valid}>Apply</button>
  </form>;
}
function FlowEditor({element,disabled,commit}:{element:Element;disabled:boolean;commit:(e:Element)=>void}){
  const [spacing,setSpacing]=useState(String(element.spacing??0));
  const [alignment,setAlignment]=useState<Cross>(element.alignment??"start");
  const [arrangement,setArrangement]=useState<Arrangement>(element.arrangement??"start");
  const [wrap,setWrap]=useState(element.wrap??false);
  const gap=parseFixed(spacing);
  return <form className="layout-field" onSubmit={e=>{e.preventDefault();if(!disabled&&gap!==undefined){const next={...element,spacing:gap,alignment,arrangement,wrap};setSpacing(String(element.spacing??0));setAlignment(element.alignment??"start");setArrangement(element.arrangement??"start");setWrap(element.wrap??false);commit(next);}}}>
    <label>Gap (dp)<input type="number" min="0" step="any" value={spacing} disabled={disabled} onChange={e=>setSpacing(e.currentTarget.value)}/></label>
    <label>Main axis<select value={arrangement} disabled={disabled} onChange={e=>setArrangement(e.currentTarget.value as Arrangement)}>{(["start","center","end","spaceBetween","spaceAround","spaceEvenly"] as const).map(a=><option key={a}>{a}</option>)}</select></label>
    <label>Cross axis<select value={alignment} disabled={disabled} onChange={e=>setAlignment(e.currentTarget.value as Cross)}>{(["start","center","end"] as const).map(a=><option key={a}>{a}</option>)}</select></label>
    <label className="check-line"><input type="checkbox" checked={wrap} disabled={disabled} onChange={e=>setWrap(e.currentTarget.checked)}/> Wrap children</label>
    <button type="submit" disabled={disabled||gap===undefined}>Apply arrangement</button>
  </form>;
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
type Glyph = "folder"|"save"|"undo"|"redo"|"plus"|"layers"|"palette"|"inspector"|"terminal"|"page"|"play"|"box"|"check"|"up"|"down"|"trash"|"text"|"button"|"gauge"|"row"|"column";
const glyphs:Record<Glyph,string>={
  folder:"M3 6h7l2 2h9v11H3z",save:"M4 4h13l3 3v13H4zM8 4v6h8V4M8 20v-6h8v6",
  undo:"M8 5 3 10l5 5M3 10h11a5 5 0 0 1 0 10",redo:"m16 5 5 5-5 5m5-5H10a5 5 0 0 0 0 10",
  plus:"M12 5v14M5 12h14",layers:"m12 3 9 5-9 5-9-5 9-5m-9 9 9 5 9-5m-18 5 9 5 9-5",
  palette:"M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 17h7m-3.5-3.5v7",
  inspector:"M4 6h16M4 12h16M4 18h16M9 4v4M15 10v4M7 16v4",
  terminal:"M3 5h18v14H3zM7 9l3 3-3 3m6 0h4",page:"M3 4h18v16H3zM3 8h18m-11 0v12",
  play:"m8 4 12 8-12 8z",box:"m12 3 9 4-9 4-9-4 9-4m9 4v10l-9 4-9-4V7m9 4v10",
  check:"m4 12 5 5L20 6",up:"m6 15 6-6 6 6",down:"m6 9 6 6 6-6",trash:"M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7m4-7v7",
  text:"M5 6h14M12 6v13m-4 0h8",button:"M3 7h18v10H3zM8 12h8",
  gauge:"M5 18a9 9 0 1 1 14 0H5Zm7-4 4-5",row:"M3 5h18v14H3zM9 5v14m6-14v14",
  column:"M5 3h14v18H5zM5 9h14m-14 6h14"
};
function Icon({name}:{name:Glyph}){return <svg className="studio-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={glyphs[name]}/></svg>;}
function Studio(){
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null);
  const [status,setStatus]=useState("Connecting to engineering core…");
  const [menu,setMenu]=useState<string|null>(null);
  const [busy,setBusy]=useState(false);
  const [tabs,setTabs]=useState<Tabs>({open:[],active:null});
  const [selected,setSelected]=useState<string|null>(null);
  const [panels,setPanels]=useState<Preferences>(readPanels);
  const [leftView,setLeftView]=useState<"project"|"layers"|"components">("project");
  const [outputView,setOutputView]=useState<"diagnostics"|"status">("diagnostics");
  const [manualZoom,setManualZoom]=useState<number|null>(null);
  const [fitZoom,setFitZoom]=useState(1);
  const viewportRef=useRef<HTMLDivElement>(null);
  const zoom=manualZoom??fitZoom;
  useEffect(()=>{
    const element=viewportRef.current;
    if(!element)return;
    const fit=()=>setFitZoom(fitPreview(element.clientWidth,element.clientHeight));
    const observer=new ResizeObserver(fit);
    observer.observe(element);fit();
    return()=>observer.disconnect();
  },[]);
  const [tab,setTab]=useState<"properties"|"layout"|"bindings"|"events">("properties");
  const inFlight=useRef(false);
  const generation=useRef<number|null>(null);
  const [pendingRecovery,setPendingRecovery]=useState(true);
  const [autosaveMessage,setAutosaveMessage]=useState("Checking recovery…");
  const screen=snapshot?.project.screens.find(s=>s.id===tabs.active);
  const chosen=screen?findNode(screen.root,selected):undefined;
  const send=useCallback(async(request:Request)=>{
    if(inFlight.current){setStatus("Another engineering command is pending; retry after it finishes.");return;}
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<Reply>("engineering_request",{request,generation:generation.current});
      generation.current=reply.generation;
      setSnapshot(reply.snapshot);
      setTabs(t=>{const ids=reply.snapshot.project.screens.map(s=>s.id);const next=reconcileTabs(t,ids);return request.request==="snapshot"&&ids.length&&!next.open.length?openTab(next,ids[0]):next;});
      if(reply.response.response==="error")setStatus(reply.response.error?.message??"Command rejected");
      else {setStatus("Engineering core ready · revision "+reply.snapshot.revision);const ids=reply.response.outcome?.allocatedIds;if(ids?.["0:screen"]){setTabs(t=>openTab(t,ids["0:screen"]));setSelected(null);}if(ids?.["0:node:pending"])setSelected(ids["0:node:pending"]);}
    }catch(e){setStatus("Command failed: "+String(e));}
    finally{inFlight.current=false;setBusy(false);}
  },[]);
  useEffect(()=>{void send({request:"snapshot"});},[send]);
  useEffect(()=>{
    let mounted=true;
    async function refresh(){
      if(inFlight.current)return;
      try{
        const p=await invoke<PersistenceStatus>("persistence_status");
        if(mounted&&!inFlight.current){setPendingRecovery(p.pendingUntitled);setAutosaveMessage(p.autosaveMessage);}
      }catch{/* native dialog or autosave in progress; retry */}
    }
    void refresh();
    const timer=window.setInterval(()=>void refresh(),2000);
    return()=>{mounted=false;window.clearInterval(timer);};
  },[snapshot]);
  async function projectAction(action:ProjectAction){
    if(inFlight.current)return;
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<ProjectReply>("project_action",{action});
      generation.current=reply.generation;
      setSnapshot(reply.snapshot);
      setPendingRecovery(reply.pendingUntitled);
      if(reply.replaced){
        const first=reply.snapshot.project.screens[0]?.id;
        setTabs(first?{open:[first],active:first}:{open:[],active:null});
        setSelected(null);
      }
      setStatus(reply.message);
    }catch(e){
      setStatus("Project operation failed: "+String(e));
      // A save may have succeeded before the subsequent open failed. Re-read
      // authoritative status without resetting existing screen tabs.
      try{
        const reply=await invoke<Reply>("engineering_request",{request:{request:"snapshot"},generation:generation.current});
        generation.current=reply.generation;setSnapshot(reply.snapshot);
      }catch{/* preserve the existing view until the host becomes available */}
    }finally{inFlight.current=false;setBusy(false);}
  }
  useEffect(()=>{try{localStorage.setItem(STORAGE_KEY,JSON.stringify(panels));}catch{/* only presentation preferences */}},[panels]);
  const execute=(command:Command,label:string)=>{if(snapshot&&!pendingRecovery)void send({request:"execute",transaction:{commands:[command],expectedRevision:snapshot.revision,label}})};
  const history=useCallback((request:"undo"|"redo")=>{if(snapshot && (request==="undo"?snapshot.status.canUndo:snapshot.status.canRedo))void send({request,expectedRevision:snapshot.revision});},[snapshot,send]);
  const toggle=(key:Panel)=>setPanels(p=>({...p,[key]:!p[key]}));
  const resize=(key:SizeKey,n:number)=>setPanels(p=>({...p,[key]:clampSize(n)}));
  const activate=(id:string)=>{if(!inFlight.current){setTabs(t=>openTab(t,id));setSelected(null);}};
  const close=(id:string)=>{if(!inFlight.current){setTabs(t=>closeTab(t,id));if(tabs.active===id)setSelected(null);}};
  const cycle=(delta:number)=>{if(!inFlight.current){setTabs(t=>cycleTab(t,delta));setSelected(null);}};
  const parent=screen?insertionParent(screen.root,chosen?.id??null):undefined;
  const earlier=screen&&chosen?siblingDestination(screen.root,chosen.id,-1):undefined;
  const later=screen&&chosen?siblingDestination(screen.root,chosen.id,1):undefined;
  const insert=(kind:"button"|"text"|"gauge"|"row"|"column")=>{
    if(!parent)return;
    const node:Node=kind==="row"||kind==="column"
      ?{id:"pending",name:kind==="row"?"Row":"Column",element:{type:kind}}
      :{id:"pending",element:{type:"component",kind},properties:kind==="gauge"?{value:0,min:0,max:100}:{text:kind==="button"?"Button":"Text"}};
    execute({op:"insertNode",parent,node,assignNewIds:true},"Insert "+kind);
  };
  const reorder=(index:number|undefined)=>{
    if(chosen&&index!==undefined)execute({op:"reorderNode",node:chosen.id,index},"Reorder layer");
  };
  type Action = {run:()=>void; enabled:boolean; shortcut?:string; checked?:boolean};
  const ready=!!snapshot&&!busy&&!pendingRecovery;
  const actions:Record<string,Action>={
    "New screen":{run:()=>execute({op:"createScreen",name:"Screen "+((snapshot?.project.screens.length??0)+1)},"New screen"),enabled:ready},
    "Undo":{run:()=>history("undo"),enabled:ready&&!!snapshot?.status.canUndo,shortcut:"Mod+Z"},
    "Redo":{run:()=>history("redo"),enabled:ready&&!!snapshot?.status.canRedo,shortcut:"Mod+Shift+Z"},
    "Delete selection":{run:()=>{if(chosen)execute({op:"removeNode",node:chosen.id},"Delete selection");},enabled:ready&&!!chosen&&chosen.id!==screen?.root.id,shortcut:"Delete"},
    "Button":{run:()=>insert("button"),enabled:ready&&parent!==undefined},
    "Text":{run:()=>insert("text"),enabled:ready&&parent!==undefined},
    "Gauge":{run:()=>insert("gauge"),enabled:ready&&parent!==undefined},
    "Row":{run:()=>insert("row"),enabled:ready&&parent!==undefined},
    "Column":{run:()=>insert("column"),enabled:ready&&parent!==undefined},
    "Move earlier":{run:()=>reorder(earlier),enabled:ready&&earlier!==undefined},
    "Move later":{run:()=>reorder(later),enabled:ready&&later!==undefined},
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
    "New project":{run:()=>void projectAction("newProject"),enabled:ready,shortcut:"Mod+N"},
    "Open project":{run:()=>void projectAction("open"),enabled:ready,shortcut:"Mod+O"},
    "Save project":{run:()=>void projectAction("save"),enabled:ready,shortcut:"Mod+S"},
    "Save project as":{run:()=>void projectAction("saveAs"),enabled:ready,shortcut:"Mod+Shift+S"},
    "Recover project":{run:()=>void projectAction("recover"),enabled:ready},
    "Resolve untitled recovery":{run:()=>void projectAction("resolveUntitled"),enabled:!!snapshot&&!busy&&pendingRecovery},
    "Exit":{run:()=>void projectAction("exit"),enabled:!busy}
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
  const menus:Record<string,string[]>={File:["New project","Open project","Save project","Save project as","Recover project","Resolve untitled recovery","New screen","Close tab","Exit"],Edit:["Undo","Redo","Delete selection"],View:["Project explorer","Inspector","Diagnostics","Swap side docks","Standard layout","Design layout","Diagnostics layout","Reset workspace","Next tab","Previous tab"],Insert:["Button","Text","Gauge","Row","Column"],Arrange:["Move earlier","Move later","Align (planned)"],Project:["Validate project"],PLC:["Connections (planned)"],Simulation:["Preview (planned)"],Runtime:["Export (planned)","Publish (planned)"],Tools:["Preferences (planned)"],Help:["About (planned)"]};
  return <main className="studio" onClick={()=>setMenu(null)}>
    <header className="studio-top"><div className="brand"><span className="brand-mark"><Icon name="box"/></span><strong>HMI <em>Studio</em><small>ENGINEERING</small></strong></div><span className="header-project" title={snapshot?.status.path??"Untitled project"}><Icon name="folder"/>{snapshot?.project.metadata.name??"Loading"}{snapshot?.status.dirty?" *":""}</span><span className="grow"/><small className="build-label">DEVELOPMENT</small><div className="top-actions"><button disabled title="Runtime preview planned"><Icon name="play"/> Preview</button><button disabled title="Simulation UI planned">Simulation</button><button className="primary" disabled title="Publishing planned"><Icon name="box"/> Publish</button></div></header>
    <nav aria-label="Main menu" onClick={e=>e.stopPropagation()}>{Object.entries(menus).map(([name,items])=><div className="menu" key={name}><button aria-expanded={menu===name} onClick={()=>setMenu(menu===name?null:name)}>{name}</button>{menu===name&&<div className="dropdown">{items.map(item=><button disabled={disabled(item)} key={item} onClick={()=>action(item)}>{item}{actions[item]?.checked?" ✓":""}{actions[item]?.shortcut&&<small className="shortcut">{actions[item].shortcut}</small>}</button>)}</div>}</div>)}</nav>
    <div className="toolbar ribbon" aria-label="Engineering toolbar">
      {([[["Open project","folder"],["Save project","save"],["Undo","undo"],["Redo","redo"]],[["New screen","plus"],["Delete selection","trash"],["Move earlier","up"],["Move later","down"]],[["Validate project","check"]]] as [string,Glyph][][]).map((group,i)=><div className="tool-group" key={i}>{group.map(([name,icon])=><button key={name} title={name} aria-label={name} disabled={disabled(name)} onClick={()=>action(name)}><Icon name={icon}/></button>)}</div>)}
      <div className="tool-group"><button onClick={()=>{setLeftView("components");setPanels(p=>({...p,left:true}));}} disabled={busy}><Icon name="plus"/> Insert</button><button disabled title="Pan and snapping planned for direct manipulation">Pan / snap (planned)</button></div><div className="tool-group zoom-tools"><button aria-label="Zoom out" title="Zoom out" disabled={!screen||zoom<=.25} onClick={()=>setManualZoom(changeZoom(zoom,-.1))}>−</button><button aria-label="Fit preview" title="Fit preview" disabled={!screen} onClick={()=>setManualZoom(null)}>{Math.round(zoom*100)}%</button><button aria-label="Zoom in" title="Zoom in" disabled={!screen||zoom>=2} onClick={()=>setManualZoom(changeZoom(zoom,.1))}>+</button></div>
      <span className="grow"/><span className="workspace-label">Workspace</span><div className="tool-group">{([["Project explorer","layers"],["Diagnostics","terminal"],["Inspector","inspector"]] as [string,Glyph][]).map(([name,icon])=><button key={name} title={name} aria-label={name} disabled={disabled(name)} aria-pressed={actions[name].checked} onClick={()=>action(name)}><Icon name={icon}/></button>)}</div>
    </div>
    {pendingRecovery&&snapshot&&<div className="recovery-banner" role="status">Resolve untitled recovery before editing. <button disabled={busy} onClick={()=>action("Resolve untitled recovery")}>Review recovery</button></div>}
    <section className={"workspace dock-workspace "+(panels.swapped?"swapped":"")}>
    <aside className="activity-rail" aria-label="Workspace tools">{([["project","folder","Project"],["layers","layers","Layers"],["components","palette","Components"]] as const).map(([view,icon,label])=><button key={view} aria-label={label} title={label} aria-pressed={panels.left&&leftView===view} onClick={()=>{setLeftView(view);setPanels(p=>({...p,left:!(p.left&&leftView===view)}));}}><Icon name={icon}/></button>)}<span className="grow"/><button aria-label="Diagnostics" title="Diagnostics" aria-pressed={panels.diagnostics} onClick={()=>action("Diagnostics")}><Icon name="terminal"/></button></aside>
    {panels.left&&<aside className={"left dock-panel size-"+panels.explorerSize}><h3>{leftView==="project"?"PROJECT EXPLORER":leftView==="layers"?"LAYERS":"COMPONENT LIBRARY"}</h3>{leftView==="project"&&<><div className="dock-heading">SCREENS</div><div className="tree">{snapshot?.project.screens.map(s=><button key={s.id} className={"layer "+(screen?.id===s.id?"active":"")} disabled={busy} onClick={()=>activate(s.id)}><Icon name="page"/> {s.name}</button>)}</div><button className="dock-add" disabled={disabled("New screen")} onClick={()=>action("New screen")}><Icon name="plus"/> New screen</button><div className="dock-heading">LAYERS</div>{screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</>}{leftView==="components"&&<div className="component-library">{([["Button","button"],["Text","text"],["Gauge","gauge"],["Row","row"],["Column","column"]] as [string,Glyph][]).map(([name,glyph])=><button key={name} disabled={disabled(name)} onClick={()=>action(name)}><Icon name={glyph}/>{name}</button>)}</div>}{leftView==="layers"&&screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</aside>}
    {panels.left&&<div className="explorer-split"><Splitter label="Resize explorer" size={panels.explorerSize} vertical direction={panels.swapped?-1:1} change={n=>resize("explorerSize",n)}/></div>}
    <section className="designer"><div className="screen-tabs" aria-label="Open screens">{tabs.open.map(id=>{const s=snapshot?.project.screens.find(x=>x.id===id);return s&&<div className="screen-tab" key={id}><button aria-current={tabs.active===id?"page":undefined} className={tabs.active===id?"active":""} disabled={busy} onClick={()=>activate(id)}>{s.name}</button><button aria-label={"Close "+s.name} disabled={busy} onClick={()=>close(id)}>×</button></div>;})}</div><div className="designer-bar"><strong>{screen?.name??"No screen"}</strong><span>Base layout · 800 × 480 dp</span></div><div className="viewport" ref={viewportRef}>{!screen&&<p className="muted">Open a screen in Project Explorer or create a new screen.</p>}{screen&&<div className="artboard-wrap" style={{width:PREVIEW_WIDTH*zoom}}><div className="artboard-caption">{parent?"Insert into "+label(findNode(screen.root,parent)??screen.root):"Select a row or column to insert"}<span>{countLimits(screen.root)} preview limitations</span></div><div className="artboard-scale" style={{width:PREVIEW_WIDTH*zoom,height:PREVIEW_HEIGHT*zoom}}><div className="artboard-logical" style={{transform:"scale("+zoom+")"}}><div className="artboard-ruler" aria-hidden="true">{[0,100,200,300,400,500,600,700].map(n=><span key={n}>{n}</span>)}</div><div className="artboard-row"><div className="artboard-ruler-y" aria-hidden="true">{[0,100,200,300,400].map(n=><span key={n}>{n}</span>)}</div><div className="artboard"><Preview node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/></div></div></div></div><p className="preview-note">Base semantic flow preview · static components · responsive, non-flow and live semantics not evaluated. Not runtime parity.</p></div>}</div><div className="canvas-foot"><Icon name="page"/><span>{screen?.name??"No screen"}</span><span className="muted">{chosen?label(chosen):"No selection"}</span><span className="grow"/><span>800 × 480 dp · {Math.round(zoom*100)}%</span></div></section>
    {panels.right&&<div className="inspector-split"><Splitter label="Resize inspector" size={panels.inspectorSize} vertical direction={panels.swapped?1:-1} change={n=>resize("inspectorSize",n)}/></div>}
    {panels.right&&<aside className={"right dock-panel size-"+panels.inspectorSize}><h3>INSPECTOR</h3><div className="inspector-tabs">{(["properties","layout","bindings","events"] as const).map(t=><button key={t} className={tab===t?"active":""} onClick={()=>setTab(t)}>{t}</button>)}</div>{tab==="properties"?<>{screen&&<EditField key={generation.current+":"+screen.id+":"+snapshot?.revision} label="Screen name" value={screen.name} disabled={busy||pendingRecovery} normalize={v=>v.trim()} commit={name=>execute({op:"renameScreen",screen:screen.id,name},"Rename screen")}/>}{chosen?<><dl><dt>ID</dt><dd>{chosen.id}</dd><dt>Type</dt><dd>{chosen.element.kind??chosen.element.type}</dd></dl>{(chosen.element.kind==="text"||chosen.element.kind==="button")&&<EditField key={generation.current+":"+chosen.id+":"+snapshot?.revision} label="Text" value={typeof chosen.properties?.text==="string"?chosen.properties.text:""} disabled={busy||pendingRecovery||typeof chosen.properties?.text==="object"} commit={value=>execute({op:"setProperty",node:chosen.id,name:"text",value},"Edit text")}/>}</>:<p>Select a layer.</p>}</>:tab==="layout"?chosen?<><h3>SIZE / FLOW</h3>{(isFlow(chosen.element)||chosen.element.type==="component")?<div key={generation.current+":"+chosen.id+":"+snapshot?.revision}><SizeEditor axis="Width" size={chosen.layout?.width} disabled={!ready} commit={width=>execute({op:"setLayout",node:chosen.id,layout:{...chosen.layout,width}},"Set width")}/><SizeEditor axis="Height" size={chosen.layout?.height} disabled={!ready} commit={height=>execute({op:"setLayout",node:chosen.id,layout:{...chosen.layout,height}},"Set height")}/>{isFlow(chosen.element)&&<FlowEditor element={chosen.element} disabled={!ready} commit={element=>execute({op:"setElement",node:chosen.id,element},"Set flow arrangement")}/>}</div>:<p>Layout editing is not available for this element.</p>}{previewLimit(chosen).map(l=><p className="layout-warning" key={l}>{l}</p>)}</>:<p>Select a layer to edit its layout.</p>:<p>Editing {tab} is planned.</p>}</aside>}
    </section>
    {panels.diagnostics&&<Splitter label="Resize diagnostics" size={panels.diagnosticsSize} vertical={false} direction={-1} change={n=>resize("diagnosticsSize",n)}/>}
    {panels.diagnostics&&<section className={"diagnostics dock-diagnostics size-"+panels.diagnosticsSize}><div className="output-tabs" aria-label="Engineering output"><button aria-pressed={outputView==="diagnostics"} onClick={()=>setOutputView("diagnostics")}><Icon name="terminal"/> Diagnostics <span>{snapshot?.status.errorCount??0} errors · {snapshot?.status.warningCount??0} warnings</span></button><button aria-pressed={outputView==="status"} onClick={()=>setOutputView("status")}>Status</button></div>{outputView==="status"?<p>{status}</p>:<>{snapshot?.diagnostics.length?snapshot.diagnostics.map((d,i)=><p key={i}>{d.severity} · {d.code} · {d.path} · {d.message}</p>):<p>No diagnostics reported.</p>}</>}</section>}
    <footer><span className="no-plc">● No live PLC</span><span role="status">{status}</span><span className="grow"/><span>{(snapshot?.status.dirty?"Unsaved changes":"No unsaved edits")+" · "+autosaveMessage}</span><span title="Planned runtime targets">Windows · Linux · Android</span></footer>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Studio/>);
