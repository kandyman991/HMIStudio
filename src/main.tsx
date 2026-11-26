import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";
import { clampSize, restorePreferences, preset, DEFAULTS, STORAGE_KEY, reconcileTabs, openTab, closeTab, cycleTab, matchesCatalog } from "./workspace";
import type { Preferences, Tabs, Panel } from "./workspace";
import type { PointerEvent as ReactPointerEvent } from "react";
import { insertionParent, siblingDestination, canDrag, canResize, canAlign, planDrop, flowGapDrop, emptyFlowMin, resizeGripHit, resizeDimensions, sameGestureContext } from "./designer";
import type { DropPlan, DropZone, Dimensions, GestureContext } from "./designer";
import { nodeStyle, previewLimit, isFlow, parseFixed, fitPreview, changeZoom, PREVIEW_WIDTH, PREVIEW_HEIGHT } from "./layout-preview";
import type { Element, Layout, Size, Arrangement, Cross } from "./layout-preview";
import type { Contract, Kind, V2Project } from "./v2";
import { contractFor, defaultContract, defaultLayout, kindLabel, KINDS } from "./v2";
import { ContractEditor, ContractPreview } from "./v2-components";

type Node = { id: string; name?: string; element: Element; layout?: Layout; responsive?: unknown[]; bindings?: unknown[]; style?: string; events?: unknown[]; children?: Node[]; properties?: Record<string, unknown> };
type Screen = { id: string; name: string; root: Node };
type Snapshot = { revision: number; project: V2Project & { metadata: { name: string }; screens: Screen[] }; diagnostics: { severity: string; code: string; path: string; message: string }[]; status: { dirty: boolean; canUndo: boolean; canRedo: boolean; undoLabel?: string; redoLabel?: string; errorCount: number; warningCount: number; path?: string | null; autosaveCurrent: boolean } };
type Command =
  | { op: "insertNode"; parent: string; node: Node; assignNewIds: true }
  | { op: "insertV2Component"; parent: string; layout: Layout; contract: Contract }
  | { op: "setV2ComponentContract"; node: string; contract: Contract }
  | { op: "duplicateNode"; node: string }
  | { op: "removeNode"; node: string }
  | { op: "reorderNode"; node: string; index: number }
  | DropPlan
  | { op: "setLayout"; node: string; layout: Layout }
  | { op: "setElement"; node: string; element: Element }
  | { op: "createScreen"; name: string }
  | { op: "renameScreen"; screen: string; name: string }
  | { op: "setProperty"; node: string; name: string; value: string };
type Request = { request: "snapshot" | "validate" } | { request: "undo" | "redo"; expectedRevision: number } | { request: "execute"; transaction: { commands: Command[]; expectedRevision: number; label: string } };
type Reply = { generation: number; response: { response: string; error?: { message: string }; outcome?: { allocatedIds: Record<string,string> } }; snapshot: Snapshot };
type ProjectAction = "open" | "importLegacy" | "openReview" | "save" | "saveAs" | "recover" | "resolveUntitled" | "exit" | "newProject";
type ProjectReply = { snapshot: Snapshot; generation: number; replaced: boolean; message: string; persistence: PersistenceStatus };
type PersistenceStatus = { generation: number; pendingUntitled: boolean; autosaveMessage: string; reviewRequired: boolean };
type SimulationPoint = { id:string; name:string; value:string|null; writable:boolean; momentary:boolean };
type Simulation = { session:number; points:SimulationPoint[] };
type SimulationOutcome = {status:"active"; simulation:Simulation}
  | {status:"stopped" | "busy"}
  | {status:"rejected"; message:string; simulation:Simulation|null}
  | {status:"lost"; message:string};
type SimulationRequest = { action:"start"; revision:number }
  | { action:"read" | "stop"; session:number }
  | { action:"write"; session:number; tag:string; value:string }
  | { action:"pulse"; session:number; tag:string };

// A lost IPC response must not hold the workspace busy forever. A late reply
// is ignored. Only reads are retried by the periodic refresh.
async function invokeSimulation(request:SimulationRequest,generation:number):Promise<SimulationOutcome>{
  let timer:number|undefined;
  try{
    const timeout=new Promise<never>((_,reject)=>{
      timer=window.setTimeout(()=>reject(new Error("No native response. Operation outcome unknown.")),15000);
    });
    return await Promise.race([invoke<SimulationOutcome>("simulation_request",{request,generation}),timeout]);
  }finally{window.clearTimeout(timer);}
}

function SimulatedTag({point,disabled,write,pulse}:{point:SimulationPoint;disabled:boolean;write:(value:string)=>void;pulse:()=>void}){
  const [value,setValue]=useState("");
  return <form className="field" onSubmit={event=>{event.preventDefault();if(!disabled&&value.trim())write(value);}}>
    <label>{point.name} <output>{point.value??"Write only"}</output>
      {point.writable&&!point.momentary&&<input aria-label={point.name+" JSON value"} placeholder="JSON value" value={value} disabled={disabled} onChange={event=>setValue(event.currentTarget.value)}/>}
    </label>
    {point.momentary?<button type="button" onClick={pulse} disabled={disabled}>Pulse</button>:point.writable&&<button type="submit" disabled={disabled||!value.trim()}>Write</button>}
  </form>;
}
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
function Preview({node,selected,select,parent,resizable,mark,project}:{node:Node;selected:string|null;select:(id:string)=>void;parent?:Element;resizable:string|null;mark?:{id:string;zone:DropZone};project:V2Project}) {
  const kind=node.element.kind??node.element.type, contract=contractFor(project,node.id), limits=previewLimit(node);
  const style=nodeStyle(node,parent);
  if(contract){const defaults=defaultLayout(contract.kind);if(!node.layout?.width||node.layout.width.mode==="auto")style.width=defaults.width?.mode==="fixed"?defaults.width.value:undefined;if(!node.layout?.height||node.layout.height.mode==="auto")style.height=defaults.height?.mode==="fixed"?defaults.height.value:undefined;}

  const emptyMin=emptyFlowMin(node);
  if(emptyMin.minWidth!==undefined)style.minWidth=emptyMin.minWidth;
  if(emptyMin.minHeight!==undefined)style.minHeight=emptyMin.minHeight;
  // Selection chrome is an outline, never a participating child of the flow.
  const cls="preview-node"+(selected===node.id?" preview-selected":"");
  const attrs={"data-node-id":node.id,"data-resizable":resizable===node.id?"true":undefined,"data-drop":mark?.id===node.id?mark.zone:undefined};
  if(isFlow(node.element)||node.element.type==="absolute"||node.element.type==="box")return <div {...attrs} className={cls+" preview-flow"} tabIndex={-1} style={style} title={label(node)+(limits.length?" · "+limits.join("; "):"")} onClick={e=>{e.stopPropagation();select(node.id);}}>{node.children?.map(n=><Preview key={n.id} node={n} parent={node.element} selected={selected} select={select} resizable={resizable} mark={mark} project={project}/>)}</div>;
  if(contract)return <button {...attrs} type="button" className={cls+" preview-component v2-preview"} style={style} title={kindLabel[contract.kind]+" · static design preview; no runtime I/O"} onClick={e=>{e.stopPropagation();select(node.id);}}><ContractPreview contract={contract} project={project}/></button>;
  // Keep descendants reachable in unsupported layout containers. This is
  // an explicitly approximate engineering view, not inferred placement.
  if(node.children?.length)return <div {...attrs} className={cls+" preview-flow preview-fallback"} style={style} onClick={e=>{e.stopPropagation();select(node.id);}}><small>Approximate layout · {kind}</small>{node.children.map(n=><Preview key={n.id} node={n} parent={node.element} selected={selected} select={select} resizable={null} project={project}/>)}</div>;
  if(node.element.type!=="component")return <button {...attrs} type="button" className={cls+" preview-unsupported"} style={style} onClick={e=>{e.stopPropagation();select(node.id);}} title={limits.join("; ")}>Unsupported · {label(node)} (see layers)</button>;
  return <button {...attrs} type="button" className={cls+" preview-component "+(["text","button","gauge"].includes(kind)?kind:"placeholder")} style={style} title={limits.join("; ")||label(node)} onClick={e=>{e.stopPropagation();select(node.id);}}>{kind==="gauge"?<><small>Static gauge</small><strong>{String(node.properties?.value??0)}</strong></>:kind==="text"||kind==="button"?(typeof node.properties?.text==="string"?node.properties.text:node.properties?.text!==undefined?"[Unresolved text]":label(node)):"Placeholder · "+kind}</button>;
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
type Glyph = "folder"|"save"|"undo"|"redo"|"plus"|"layers"|"palette"|"inspector"|"terminal"|"page"|"play"|"box"|"check"|"up"|"down"|"trash"|"text"|"button"|"gauge"|"row"|"column"|"search"|"cursor"|"hand"|"grid"|"snap"|"fit"|"close"|"chevron"|"tag"|"bell";
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
  column:"M5 3h14v18H5zM5 9h14m-14 6h14",search:"M15 15l6 6M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0",cursor:"M5 3l14 10-7 1-3 6z",hand:"M8 13V6a2 2 0 0 1 4 0v5-6a2 2 0 0 1 4 0v6-3a2 2 0 0 1 4 0v7a6 6 0 0 1-6 6h-2l-6-6z",grid:"M4 4h16v16H4zM9 4v16m6-16v16M4 9h16M4 15h16",snap:"M5 5h4v9a3 3 0 0 0 6 0V5h4v9a7 7 0 0 1-14 0z",fit:"M9 4H4v5m11-5h5v5M4 15v5h5m6 0h5v-5M8 8h8v8H8z",close:"M6 6l12 12M18 6L6 18",chevron:"m9 6 6 6-6 6",tag:"M3 4h8l10 10-7 7L3 10zM7 8h.1",bell:"M5 17h14l-2-3V9a5 5 0 0 0-10 0v5zm5 3h4"
};
function Icon({name}:{name:Glyph}){return <svg className="studio-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={glyphs[name]}/></svg>;}
function Studio(){
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null);
  const [status,setStatus]=useState("Connecting to engineering core…");
  const [menu,setMenu]=useState<string|null>(null);
  const [submenu,setSubmenu]=useState<string|null>(null);
  const [catalogQuery,setCatalogQuery]=useState("");
  const [commandQuery,setCommandQuery]=useState("");
  const [grid,setGrid]=useState(true);
  const navRef=useRef<HTMLElement>(null);
  const commandRef=useRef<HTMLInputElement>(null);
  const [busy,setBusy]=useState(false);
  const [tabs,setTabs]=useState<Tabs>({open:[],active:null});
  const [selected,setSelected]=useState<string|null>(null);
  const [panels,setPanels]=useState<Preferences>(readPanels);
  const [leftView,setLeftView]=useState<"project"|"layers"|"components">("project");
  const [outputView,setOutputView]=useState<"diagnostics"|"status"|"simulation">("diagnostics");
  const [simulation,setSimulation]=useState<Simulation|null>(null);
  const [simulationUnavailable,setSimulationUnavailable]=useState(false);
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
  const [reviewRequired,setReviewRequired]=useState(false);
  useEffect(()=>{setSimulation(null);setSimulationUnavailable(false);},[generation.current,snapshot?.revision]);
  const simulate=useCallback(async(request:SimulationRequest,background=false)=>{
    if(inFlight.current||generation.current===null||pendingRecovery)return;
    inFlight.current=true;
    setBusy(true);
    try{
      const reply=await invokeSimulation(request,generation.current);
      switch(reply.status){
        case "active":
          setSimulation(reply.simulation);
          setSimulationUnavailable(false);
          if(request.action==="start"){
            setPanels(p=>({...p,diagnostics:true}));
            setOutputView("simulation");
          }
          if(!background)setStatus("Isolated simulation active");
          else setStatus(previous=>previous.startsWith("Simulation status unavailable")?"Isolated simulation active":previous);
          break;
        case "stopped":
          setSimulation(null);
          setSimulationUnavailable(false);
          setStatus("Simulation stopped");
          break;
        case "rejected":
          setSimulation(reply.simulation);
          setSimulationUnavailable(false);
          setStatus("Simulation command rejected: "+reply.message);
          break;
        case "lost":
          setSimulation(null);
          setSimulationUnavailable(false);
          setStatus("Simulation session lost: "+reply.message);
          break;
        case "busy":
          setSimulationUnavailable(true);
          if(!background)setStatus("Simulation status unavailable: project operation pending");
          break;
      }
    }catch(error){
      // An IPC failure is not proof of child exit. Keep the session identity
      // for a retry/stop, but hide its cached values until native verification.
      setSimulationUnavailable(true);
      setStatus("Simulation status unavailable: "+String(error));
    }
    finally{inFlight.current=false;setBusy(false);}
  },[pendingRecovery]);
  // Expire displayed samples even while polling is blocked by an IPC call
  // or a native dialog. Only a verified response makes them visible again.
  useEffect(()=>{
    if(!simulation)return;
    const timer=window.setTimeout(()=>setSimulationUnavailable(true),3000);
    return()=>window.clearTimeout(timer);
  },[simulation]);
  const simulationSession=simulation?.session;
  useEffect(()=>{
    if(simulationSession===undefined)return;
    const timer=window.setInterval(()=>void simulate({action:"read",session:simulationSession},true),1000);
    return()=>window.clearInterval(timer);
  },[simulationSession,simulate]);
  const updatePersistence=useCallback((p:PersistenceStatus)=>{
    setPendingRecovery(p.pendingUntitled);
    setAutosaveMessage(p.autosaveMessage);
    setReviewRequired(p.reviewRequired);
  },[]);
  const screen=snapshot?.project.screens.find(s=>s.id===tabs.active);
  const chosen=screen?findNode(screen.root,selected):undefined;
  const send=useCallback(async(request:Request)=>{
    if(inFlight.current){setStatus("Another engineering command is pending; retry after it finishes.");return;}
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<Reply>("engineering_request",{request,generation:generation.current});
      const replaced=generation.current!==reply.generation;
      generation.current=reply.generation;
      setSnapshot(reply.snapshot);
      setTabs(t=>{const ids=reply.snapshot.project.screens.map(s=>s.id);const next=reconcileTabs(replaced?{open:[],active:null}:t,ids);return (replaced||request.request==="snapshot")&&ids.length&&!next.open.length?openTab(next,ids[0]):next;});
      if(replaced)setSelected(null);
      if(reply.response.response==="error")setStatus(reply.response.error?.message??"Command rejected");
      else {setStatus("Engineering core ready · revision "+reply.snapshot.revision);const ids=reply.response.outcome?.allocatedIds;if(ids?.["0:screen"]){setTabs(t=>openTab(t,ids["0:screen"]));setSelected(null);}if(ids?.["0:node:pending"])setSelected(ids["0:node:pending"]);if(ids?.["0:node"])setSelected(ids["0:node"]);if(request.request==="execute"&&request.transaction.commands[0]?.op==="duplicateNode"){const id=ids?.["0:node:"+request.transaction.commands[0].node];if(id)setSelected(id);}}
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
        if(mounted&&!inFlight.current&&p.generation===generation.current)updatePersistence(p);
      }catch{/* native dialog or autosave in progress; retry */}
    }
    void refresh();
    const timer=window.setInterval(()=>void refresh(),2000);
    return()=>{mounted=false;window.clearInterval(timer);};
  },[snapshot,updatePersistence]);
  async function projectAction(action:ProjectAction){
    if(inFlight.current||generation.current===null)return;
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<ProjectReply>("project_action",{action,generation:generation.current});
      generation.current=reply.generation;
      setSnapshot(reply.snapshot);
      updatePersistence(reply.persistence);
      if(reply.replaced){
        const first=reply.snapshot.project.screens[0]?.id;
        setTabs(first?{open:[first],active:first}:{open:[],active:null});
        setSelected(null);
      }
      setStatus(reply.message);
    }catch(e){
      setStatus("Project operation failed: "+String(e));
      // Saving before a failed switch may have changed persistence status.
      try{
        const reply=await invoke<Reply>("engineering_request",{request:{request:"snapshot"},generation:generation.current});
        if(generation.current!==reply.generation){
          const first=reply.snapshot.project.screens[0]?.id;
          setTabs(first?{open:[first],active:first}:{open:[],active:null});
          setSelected(null);
        }
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
  const insertV2=(kind:Kind)=>{
    if(!parent||!snapshot)return;
    const contract=defaultContract(kind,snapshot.project);
    if(!contract){setStatus("Create a compatible readable/writable tag and enumeration first.");return;}
    execute({op:"insertV2Component",parent,contract,layout:defaultLayout(kind)},"Insert "+kindLabel[kind]);
  };
  const chosenContract=chosen?contractFor(snapshot?.project,chosen.id):undefined;
  const reorder=(index:number|undefined)=>{
    if(chosen&&index!==undefined)execute({op:"reorderNode",node:chosen.id,index},"Reorder layer");
  };
  type Action = {run:()=>void; enabled:boolean; shortcut?:string; checked?:boolean};
  const ready=!!snapshot&&!busy&&!pendingRecovery;
  function toggleSimulation(){
    if(simulation)void simulate({action:"stop",session:simulation.session});
    else if(snapshot)void simulate({action:"start",revision:snapshot.revision});
  }
  type Gesture = {kind:"pan";pointer:number;x:number;y:number;left:number;top:number}
    | {kind:"move"|"resize";pointer:number;x:number;y:number;node:Node;context:GestureContext;start:Dimensions;rect:{left:number;top:number;width:number;height:number};snap:boolean;viewportLeft:number;viewportTop:number;viewportWidth:number;viewportHeight:number;scrollLeft:number;scrollTop:number};
  type Feedback={mark?:{id:string;zone:DropZone};size?:Dimensions;rect?:{left:number;top:number};text:string};
  const [tool,setTool]=useState<"select"|"pan">("select");
  const [snap,setSnap]=useState(true);
  const [feedback,setFeedback]=useState<Feedback|null>(null);
  const gesture=useRef<Gesture|null>(null);
  const suppressClick=useRef(false);
  const context=():GestureContext|undefined=>screen&&snapshot?{screen:screen.id,revision:snapshot.revision,generation:generation.current,zoom}:undefined;
  function cancelGesture(){
    const g=gesture.current;gesture.current=null;setFeedback(null);
    if(g&&viewportRef.current?.hasPointerCapture(g.pointer))viewportRef.current.releasePointerCapture(g.pointer);
  }
  function currentGesture(g:Extract<Gesture,{kind:"move"|"resize"}>){
    const now=context(),v=viewportRef.current;
    if(!now||!v||!ready||inFlight.current||!sameGestureContext(g.context,now))return false;
    const bounds=v.getBoundingClientRect();
    const source=v.querySelector<HTMLElement>('[data-node-id="'+CSS.escape(g.node.id)+'"]')?.getBoundingClientRect();
    return !!source&&g.viewportLeft===bounds.left&&g.viewportTop===bounds.top&&g.viewportWidth===v.clientWidth&&g.viewportHeight===v.clientHeight&&g.scrollLeft===v.scrollLeft&&g.scrollTop===v.scrollTop&&source.left===g.rect.left&&source.top===g.rect.top&&source.width===g.rect.width&&source.height===g.rect.height;
  }
  useEffect(()=>{cancelGesture();},[screen?.id,snapshot?.revision,zoom,ready,tool]);
  useEffect(()=>{
    const blur=()=>cancelGesture();
    window.addEventListener("blur",blur);
    return()=>window.removeEventListener("blur",blur);
  },[]);
  function beginGesture(e:ReactPointerEvent<HTMLDivElement>){
    if(gesture.current||!e.isPrimary||e.button!==0)return;
    suppressClick.current=false;
    const v=e.currentTarget;
    if(tool==="pan"){
      e.preventDefault();v.setPointerCapture(e.pointerId);gesture.current={kind:"pan",pointer:e.pointerId,x:e.clientX,y:e.clientY,left:v.scrollLeft,top:v.scrollTop};suppressClick.current=true;return;
    }
    if(!screen||!ready||inFlight.current)return;
    const target=(e.target as HTMLElement).closest<HTMLElement>("[data-node-id]");
    const node=target?.dataset.nodeId?findNode(screen.root,target.dataset.nodeId):undefined,ctx=context();
    if(!target||!node||!ctx)return;
    setSelected(node.id);
    if(!canDrag(screen.root,node.id))return;
    const initialRect=target.getBoundingClientRect();
    const resize=selected===node.id&&canResize(screen.root,node.id)&&resizeGripHit(initialRect,e.clientX,e.clientY,zoom);
    e.preventDefault();target.focus({preventScroll:true});v.setPointerCapture(e.pointerId);
    const rect=target.getBoundingClientRect(),bounds=v.getBoundingClientRect();
    gesture.current={kind:resize?"resize":"move",pointer:e.pointerId,x:e.clientX,y:e.clientY,node,context:ctx,start:{width:rect.width/zoom,height:rect.height/zoom},rect:{left:rect.left,top:rect.top,width:rect.width,height:rect.height},snap,viewportLeft:bounds.left,viewportTop:bounds.top,viewportWidth:v.clientWidth,viewportHeight:v.clientHeight,scrollLeft:v.scrollLeft,scrollTop:v.scrollTop};
  }
  function proposal(g:Extract<Gesture,{kind:"move"|"resize"}>,x:number,y:number):{command?:Command;feedback:Feedback}|undefined {
    if(!screen||!currentGesture(g)||Math.hypot(x-g.x,y-g.y)<4)return;
    const viewport=viewportRef.current!;
    const bounds=viewport.getBoundingClientRect();
    if(x<bounds.left||x>=bounds.right||y<bounds.top||y>=bounds.bottom)return;
    if(g.kind==="resize"){
      const size=resizeDimensions(g.start,x-g.x,y-g.y,g.context.zoom,g.snap,g.node.layout);
      if(!size)return;
      const original=g.node.layout;
      const width={mode:"fixed" as const,value:size.width},height={mode:"fixed" as const,value:size.height};
      const unchanged=original?.width?.mode==="fixed"&&original.width.value===size.width&&original?.height?.mode==="fixed"&&original.height.value===size.height;
      return {command:unchanged?undefined:{op:"setLayout",node:g.node.id,layout:{...original,width,height}},feedback:{size,rect:g.rect,text:size.width+" × "+size.height+" dp"}};
    }
    const hit=document.elementFromPoint(x,y)?.closest<HTMLElement>("[data-node-id]");
    const target=hit?.dataset.nodeId?findNode(screen.root,hit.dataset.nodeId):undefined;
    if(!hit||!target||!viewport.contains(hit))return;
    const rect=hit.getBoundingClientRect(),p=hit.parentElement?.closest<HTMLElement>("[data-node-id]");
    const parentNode=p?.dataset.nodeId?findNode(screen.root,p.dataset.nodeId):undefined;
    const row=parentNode?.element.type==="row",fraction=row?(x-rect.left)/Math.max(1,rect.width):(y-rect.top)/Math.max(1,rect.height);
    const zone:DropZone=isFlow(target.element)&&(!parentNode||!target.children?.length||(fraction>=.25&&fraction<=.75))?"inside":fraction<.5?"before":"after";
    if(zone==="inside"){
      const horizontal=target.element.type==="row";
      const centers=Array.from(hit.children).flatMap(child=>{
        const id=(child as HTMLElement).dataset.nodeId;
        if(!id)return [];
        const b=child.getBoundingClientRect();
        if(!b.width||!b.height)return [];
        return [{id,center:horizontal?b.left+b.width/2:b.top+b.height/2}];
      });
      const drop=flowGapDrop(screen.root,g.node.id,target.id,horizontal?x:y,centers);
      const mark=drop?.command?{id:drop.id,zone:drop.zone}:undefined;
      const destination=mark?findNode(screen.root,mark.id):undefined;
      return {command:drop?.command,feedback:{mark,text:mark&&destination?mark.zone+" "+label(destination):"Unsupported or unchanged destination"}};
    }
    const command=planDrop(screen.root,g.node.id,target.id,zone);
    return {command,feedback:{mark:command?{id:target.id,zone}:undefined,text:command?(zone+" "+label(target)):"Unsupported or unchanged destination"}};
  }
  function moveGesture(e:ReactPointerEvent<HTMLDivElement>){
    const g=gesture.current;if(!g||g.pointer!==e.pointerId)return;
    if(g.kind==="pan"){e.currentTarget.scrollLeft=g.left-(e.clientX-g.x);e.currentTarget.scrollTop=g.top-(e.clientY-g.y);return;}
    if(!currentGesture(g)){cancelGesture();return;}
    const next=proposal(g,e.clientX,e.clientY);
    if(Math.hypot(e.clientX-g.x,e.clientY-g.y)>=4)suppressClick.current=true;
    setFeedback(next?.feedback??null);
  }
  function endGesture(e:ReactPointerEvent<HTMLDivElement>){
    const g=gesture.current;if(!g||g.pointer!==e.pointerId)return;
    const result=g.kind==="pan"?undefined:proposal(g,e.clientX,e.clientY);
    if(g.kind!=="pan"&&Math.hypot(e.clientX-g.x,e.clientY-g.y)>=4)suppressClick.current=true;
    cancelGesture();
    if(result?.command)execute(result.command,g.kind==="resize"?"Resize component":"Move component");
  }
  const resizable=tool==="select"&&ready&&screen&&chosen&&canResize(screen.root,chosen.id)?chosen.id:null;
  const align=(crossAlign:Cross)=>{if(chosen&&screen&&chosen.layout?.crossAlign!==crossAlign&&canAlign(screen.root,chosen.id))execute({op:"setLayout",node:chosen.id,layout:{...chosen.layout,crossAlign}},"Align component");};

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
    ...Object.fromEntries(KINDS.map(kind=>[kindLabel[kind],{run:()=>insertV2(kind),enabled:ready&&parent!==undefined&&!!snapshot&&!!defaultContract(kind,snapshot.project)}])),
    "Duplicate component":{run:()=>{if(chosen)execute({op:"duplicateNode",node:chosen.id},"Duplicate component");},enabled:ready&&!!chosen&&chosen.id!==screen?.root.id},
    "Move earlier":{run:()=>reorder(earlier),enabled:ready&&earlier!==undefined},
    "Move later":{run:()=>reorder(later),enabled:ready&&later!==undefined},
    "Validate project":{run:()=>void send({request:"validate"}),enabled:ready},
    "Isolated simulation":{run:toggleSimulation,enabled:ready,checked:!!simulation&&!simulationUnavailable},
    "Fit preview":{run:()=>setManualZoom(null),enabled:!!screen},
    "Zoom in":{run:()=>setManualZoom(changeZoom(zoom,.1)),enabled:!!screen&&zoom<2},
    "Zoom out":{run:()=>setManualZoom(changeZoom(zoom,-.1)),enabled:!!screen&&zoom>.25},
    "Toggle grid":{run:()=>setGrid(v=>!v),enabled:true,checked:grid},
    "Select tool":{run:()=>setTool("select"),enabled:true,checked:tool==="select"},
    "Pan tool":{run:()=>setTool("pan"),enabled:true,checked:tool==="pan"},
    "Snap size to 10 dp":{run:()=>setSnap(v=>!v),enabled:true,checked:snap},
    "Align start":{run:()=>align("start"),enabled:ready&&!!screen&&!!chosen&&canAlign(screen.root,chosen.id),checked:chosen?.layout?.crossAlign==="start"},
    "Align center":{run:()=>align("center"),enabled:ready&&!!screen&&!!chosen&&canAlign(screen.root,chosen.id),checked:chosen?.layout?.crossAlign==="center"},
    "Align end":{run:()=>align("end"),enabled:ready&&!!screen&&!!chosen&&canAlign(screen.root,chosen.id),checked:chosen?.layout?.crossAlign==="end"},
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
    "Import legacy project (review)":{run:()=>void projectAction("importLegacy"),enabled:ready},
    "Open review bundle":{run:()=>void projectAction("openReview"),enabled:ready},
    "Save project":{run:()=>void projectAction("save"),enabled:ready,shortcut:"Mod+S"},
    "Save project as":{run:()=>void projectAction("saveAs"),enabled:ready,shortcut:"Mod+Shift+S"},
    "Recover project":{run:()=>void projectAction("recover"),enabled:ready},
    "Resolve untitled recovery":{run:()=>void projectAction("resolveUntitled"),enabled:!!snapshot&&!busy&&pendingRecovery},
    "Exit":{run:()=>void projectAction("exit"),enabled:!busy}
  };
  function closeMenu(restoreFocus=false){
    if(restoreFocus&&menu)navRef.current?.querySelector<HTMLButtonElement>('button[data-menu-trigger="'+menu+'"]')?.focus();
    setMenu(null);setSubmenu(null);
  }
  function action(name:string){closeMenu(!!menu);if(!inFlight.current&&actions[name]?.enabled)actions[name].run();}
  const disabled=(name:string)=>!actions[name]?.enabled;
  useEffect(()=>{
    function keys(e:KeyboardEvent){
      if(e.defaultPrevented||e.isComposing)return;
      if(e.key==="Escape"){if(gesture.current){e.preventDefault();cancelGesture();return;}if(menu){e.preventDefault();closeMenu(true);}return;}
      if(gesture.current)return;
      if((e.ctrlKey||e.metaKey)&&!e.altKey&&!e.shiftKey&&e.key.toLowerCase()==="k"){e.preventDefault();setMenu("Commands");setSubmenu(null);setCommandQuery("");commandRef.current?.focus();return;}
      if(e.altKey||e.target instanceof HTMLInputElement||e.target instanceof HTMLTextAreaElement||e.target instanceof HTMLSelectElement||(e.target instanceof HTMLElement&&e.target.isContentEditable))return;
      const key=(e.ctrlKey||e.metaKey?"Mod+":"")+(e.shiftKey?"Shift+":"")+(e.key.length===1?e.key.toUpperCase():e.key);
      const name=key==="Mod+Y"?"Redo":Object.keys(actions).find(n=>actions[n].shortcut===key);
      if(name){e.preventDefault();if(!e.repeat)action(name);}
    }
    window.addEventListener("keydown",keys);return()=>window.removeEventListener("keydown",keys);
  });
  type MenuEntry=string|{label:string;items:string[]};
  const menus:Record<string,MenuEntry[]>={File:["New project","Open project","Import legacy project (review)","Open review bundle","Save project","Save project as","Recover project","Resolve untitled recovery","New screen","Close tab","Exit"],Edit:["Undo","Redo","Copy component (planned)","Paste component (planned)","Duplicate component","Delete selection",{label:"Arrange",items:["Move earlier","Move later","Align start","Align center","Align end"]}],View:[{label:"Docks",items:["Project explorer","Inspector","Diagnostics","Swap side docks","Reset workspace"]},{label:"Canvas",items:["Fit preview","Zoom in","Zoom out","Toggle grid","Select tool","Pan tool","Snap size to 10 dp"]},{label:"Workspace presets",items:["Standard layout","Design layout","Diagnostics layout"]},"Next tab","Previous tab"],Insert:[{label:"Components",items:["Button","Text","Gauge",...KINDS.map(k=>kindLabel[k])]},{label:"Containers",items:["Row","Column"]}],Engineering:["Tag monitor (planned)","Engineering search (planned)","Alarm monitor (planned)","Event log (planned)","Connections (planned)","Validate project"],Runtime:["Preview (planned)","Isolated simulation","Export (planned)","Publish (planned)"],Help:["About (planned)"]};
  useEffect(()=>{if(menu==="Commands")commandRef.current?.focus();},[menu]);
  // Capture outside presses even when a preview component stops click propagation.
  useEffect(()=>{
    if(!menu)return;
    const outside=(e:Event)=>{if(e.target instanceof window.Node&&!navRef.current?.contains(e.target)){setMenu(null);setSubmenu(null);}};
    document.addEventListener("pointerdown",outside,true);
    document.addEventListener("focusin",outside,true);
    return()=>{document.removeEventListener("pointerdown",outside,true);document.removeEventListener("focusin",outside,true);};
  },[menu]);
  const matchingActions=Object.keys(actions).filter(item=>matchesCatalog(item,commandQuery));
  const simulationOutput=simulationUnavailable?<p role="status">Simulation status unavailable. Cached values hidden; retry or stop the session.</p>:simulation?<section aria-label="Isolated simulated tags">
    <button disabled={busy} onClick={()=>void simulate({action:"read",session:simulation.session})}>Refresh values</button>
    {simulation.points.map(point=><SimulatedTag key={simulation.session+":"+point.id} point={point} disabled={busy}
      write={value=>void simulate({action:"write",session:simulation.session,tag:point.id,value})}
      pulse={()=>void simulate({action:"pulse",session:simulation.session,tag:point.id})}/>)}
  </section>:<p>Start isolated simulation to inspect and operate simulated tags.</p>;
  const menuAction=(item:string)=><button disabled={disabled(item)} key={item} onClick={()=>action(item)}>{item}{actions[item]?.checked?" ✓":""}{actions[item]?.shortcut&&<small className="shortcut">{actions[item].shortcut}</small>}</button>;
  const paletteItems=([["Button","button"],["Text","text"],["Gauge","gauge"],["Row","row"],["Column","column"],...KINDS.map(k=>[kindLabel[k],"box"])] as [string,Glyph][]).filter(([name])=>matchesCatalog(name,leftView==="components"?catalogQuery:""));
  const palette=<div className="component-library">{paletteItems.map(([name,glyph])=><button key={name} disabled={disabled(name)} title={["Text input","Dropdown"].includes(name)&&disabled(name)?"Requires a compatible readable/writable tag (and enumeration for dropdown) and flow parent":name} onClick={()=>action(name)}><Icon name={glyph}/>{name}</button>)}{!paletteItems.length&&<p>No matching components.</p>}</div>;
  return <main className="studio">
    <header className="studio-top"><div className="brand"><span className="brand-mark"><Icon name="box"/></span><strong>HMI <em>Studio</em><small>ENGINEERING</small></strong></div><span className="header-project" title={snapshot?.status.path??"Untitled project"}><Icon name="folder"/>{snapshot?.project.metadata.name??"Loading"}{snapshot?.status.dirty?" *":""}</span><span className="grow"/><small className="build-label">DEVELOPMENT</small><div className="top-actions"><button disabled title="Runtime preview planned"><Icon name="play"/> Preview</button><button disabled={!ready} onClick={toggleSimulation}>{simulation?"Stop simulation":"Simulation"}</button><button className="primary" disabled title="Publishing planned"><Icon name="box"/> Publish</button></div></header>
    <nav ref={navRef} aria-label="Main menu" onClick={e=>e.stopPropagation()}>{Object.entries(menus).map(([name,items])=><div className="menu" key={name}><button data-menu-trigger={name} aria-expanded={menu===name} onClick={()=>{setMenu(menu===name?null:name);setSubmenu(null);}}>{name}</button>{menu===name&&<div className="dropdown">{items.map(item=>typeof item==="string"?menuAction(item):<div className="submenu" key={item.label}><button aria-expanded={submenu===item.label} onClick={()=>setSubmenu(submenu===item.label?null:item.label)}>{item.label}<Icon name="chevron"/></button>{submenu===item.label&&<div className="dropdown nested">{item.items.map(menuAction)}</div>}</div>)}</div>}</div>)}<span className="grow"/><div className="menu command-menu"><button data-menu-trigger="Commands" aria-expanded={menu==="Commands"} onClick={()=>{setMenu(menu==="Commands"?null:"Commands");setSubmenu(null);setCommandQuery("");}}><Icon name="search"/> Find action <kbd>Ctrl K</kbd></button>{menu==="Commands"&&<div className="dropdown command-list"><input ref={commandRef} aria-label="Find action" placeholder="Search actions…" value={commandQuery} onChange={e=>setCommandQuery(e.currentTarget.value)}/>{matchingActions.map(menuAction)}{!matchingActions.length&&<p role="status">No matching actions.</p>}</div>}</div></nav>
    <div className="toolbar ribbon" aria-label="Engineering toolbar">
      {([[["Open project","folder"],["Save project","save"],["Undo","undo"],["Redo","redo"]],[["New screen","plus"],["Delete selection","trash"],["Move earlier","up"],["Move later","down"]],[["Validate project","check"]]] as [string,Glyph][][]).map((group,i)=><div className="tool-group" key={i}>{group.map(([name,icon])=><button key={name} title={name} aria-label={name} disabled={disabled(name)} onClick={()=>action(name)}><Icon name={icon}/></button>)}</div>)}
      <div className="tool-group"><button onClick={()=>{setLeftView("components");setPanels(p=>({...p,left:true}));}} disabled={busy}><Icon name="plus"/> Insert</button><button aria-pressed={tool==="select"} onClick={()=>action("Select tool")} title="Select, drag or resize supported flow components"><Icon name="cursor"/> Select</button><button aria-pressed={tool==="pan"} onClick={()=>action("Pan tool")} title="Pan scrollable workspace"><Icon name="hand"/> Pan</button><button aria-label="Toggle grid" title="Toggle grid" aria-pressed={grid} onClick={()=>action("Toggle grid")}><Icon name="grid"/></button><button aria-label="Snap size to 10 dp" aria-pressed={snap} onClick={()=>action("Snap size to 10 dp")} title="Snap resized dimensions to 10 dp"><Icon name="snap"/></button></div><div className="tool-group zoom-tools"><button aria-label="Zoom out" title="Zoom out" disabled={!screen||zoom<=.25} onClick={()=>setManualZoom(changeZoom(zoom,-.1))}>−</button><button aria-label="Fit preview" title="Fit preview" disabled={!screen} onClick={()=>setManualZoom(null)}>{Math.round(zoom*100)}%</button><button aria-label="Zoom in" title="Zoom in" disabled={!screen||zoom>=2} onClick={()=>setManualZoom(changeZoom(zoom,.1))}>+</button><button aria-label="Fit to workspace" title="Fit to workspace" disabled={!screen} onClick={()=>action("Fit preview")}><Icon name="fit"/></button></div>
      <span className="grow"/><span className="workspace-label">Workspace</span><div className="tool-group">{([["Project explorer","layers"],["Diagnostics","terminal"],["Inspector","inspector"]] as [string,Glyph][]).map(([name,icon])=><button key={name} title={name} aria-label={name} disabled={disabled(name)} aria-pressed={actions[name].checked} onClick={()=>action(name)}><Icon name={icon}/></button>)}</div>
    </div>
    {pendingRecovery&&snapshot&&<div className="recovery-banner" role="status">Resolve untitled recovery before editing. <button disabled={busy} onClick={()=>action("Resolve untitled recovery")}>Review recovery</button></div>}
    <section className={"workspace dock-workspace "+(panels.swapped?"swapped":"")}>
    <aside className="activity-rail" aria-label="Workspace tools">{([["project","folder","Project"],["layers","layers","Layers"],["components","palette","Components"]] as const).map(([view,icon,label])=><button key={view} aria-label={label} title={label} aria-pressed={panels.left&&leftView===view} onClick={()=>{setLeftView(view);setPanels(p=>({...p,left:!(p.left&&leftView===view)}));}}><Icon name={icon}/></button>)}<span className="grow"/><button aria-label="Diagnostics" title="Diagnostics" aria-pressed={panels.diagnostics} onClick={()=>action("Diagnostics")}><Icon name="terminal"/></button></aside>
    {panels.left&&<aside className={"left dock-panel size-"+panels.explorerSize}><h3 className="dock-title"><Icon name={leftView==="project"?"folder":leftView==="layers"?"layers":"palette"}/>{leftView==="project"?"Project explorer":leftView==="layers"?"Layers":"Component library"}<span className="grow"/><button aria-label="Hide project dock" onClick={()=>toggle("left")}><Icon name="close"/></button></h3>{leftView==="project"&&<><div className="dock-heading">SCREENS <span>{snapshot?.project.screens.length??0}</span></div><div className="tree"><div className="project-root"><Icon name="folder"/> {snapshot?.project.metadata.name??"Loading"}</div>{snapshot?.project.screens.map(s=><button key={s.id} className={"layer "+(screen?.id===s.id?"active":"")} disabled={busy} onClick={()=>activate(s.id)}><Icon name="page"/> {s.name}</button>)}</div><button className="dock-add" disabled={disabled("New screen")} onClick={()=>action("New screen")}><Icon name="plus"/> New screen</button><div className="dock-heading">LAYERS</div>{screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</>}{leftView==="project"&&<><div className="dock-heading">COMPONENTS</div>{palette}</>}{leftView==="components"&&<><input className="palette-filter" aria-label="Filter components" placeholder="Filter components…" value={catalogQuery} onChange={e=>setCatalogQuery(e.currentTarget.value)}/>{palette}</>}{leftView==="layers"&&screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</aside>}
    {panels.left&&<div className="explorer-split"><Splitter label="Resize explorer" size={panels.explorerSize} vertical direction={panels.swapped?-1:1} change={n=>resize("explorerSize",n)}/></div>}
    <section className="designer"><div className="screen-tabs" aria-label="Open screens">{tabs.open.map(id=>{const s=snapshot?.project.screens.find(x=>x.id===id);return s&&<div className="screen-tab" key={id}><button aria-current={tabs.active===id?"page":undefined} className={tabs.active===id?"active":""} disabled={busy} onClick={()=>activate(id)}><Icon name="page"/> {s.name}</button><button aria-label={"Close "+s.name} disabled={busy} onClick={()=>close(id)}>×</button></div>;})}<button className="tab-add" aria-label="New screen tab" title="New screen" disabled={disabled("New screen")} onClick={()=>action("New screen")}><Icon name="plus"/></button><span className="editor-head">SCREEN EDITOR</span></div><div className={"viewport"+(grid?"":" no-grid")+(tool==="pan"?" pan-tool":"")} ref={viewportRef} onScroll={()=>{const g=gesture.current;if(g&&g.kind!=="pan"&&!currentGesture(g))cancelGesture();}} onPointerDown={beginGesture} onPointerMove={moveGesture} onPointerUp={endGesture} onPointerCancel={e=>{if(gesture.current?.pointer===e.pointerId)cancelGesture();}} onLostPointerCapture={e=>{if(e.target===e.currentTarget&&gesture.current?.pointer===e.pointerId)cancelGesture();}} onDragStart={e=>e.preventDefault()} onClickCapture={e=>{if(suppressClick.current&&e.detail!==0){suppressClick.current=false;e.preventDefault();e.stopPropagation();}}}>{!screen&&<p className="muted">Open a screen in Project Explorer or create a new screen.</p>}{screen&&<div className="artboard-wrap" style={{width:PREVIEW_WIDTH*zoom}}><div className="artboard-caption">{parent?"Insert into "+label(findNode(screen.root,parent)??screen.root):"Select a row or column to insert"}<span>{countLimits(screen.root)} preview limitations</span></div><div className="artboard-scale" style={{width:PREVIEW_WIDTH*zoom,height:PREVIEW_HEIGHT*zoom}}><div className="artboard-logical" style={{transform:"scale("+zoom+")"}}><div className="artboard-ruler" aria-hidden="true">{[0,100,200,300,400,500,600,700].map(n=><span key={n}>{n}</span>)}</div><div className="artboard-row"><div className="artboard-ruler-y" aria-hidden="true">{[0,100,200,300,400].map(n=><span key={n}>{n}</span>)}</div><div className="artboard"><Preview node={screen.root} selected={selected} select={id=>{if(tool==="select"&&!inFlight.current)setSelected(id);}} resizable={resizable} mark={feedback?.mark} project={snapshot!.project}/></div></div></div></div><p className="preview-note">Engineering design preview · static components · unsupported layouts marked · expressions, security and actions not executed. Not runtime parity.</p></div>}{feedback?.size&&feedback.rect&&<div className="gesture-resize-preview" style={{left:feedback.rect.left,top:feedback.rect.top,width:feedback.size.width*zoom,height:feedback.size.height*zoom}} aria-hidden="true"/>}</div>{feedback&&<div className="gesture-feedback" role="status">{feedback.text}</div>}<div className="canvas-foot"><Icon name="page"/><span>{screen?.name??"No screen"}</span><span className="muted">{chosen?label(chosen):"No selection"}</span><span className="grow"/><span>800 × 480 dp · {Math.round(zoom*100)}%</span></div></section>
    {panels.right&&<div className="inspector-split"><Splitter label="Resize inspector" size={panels.inspectorSize} vertical direction={panels.swapped?1:-1} change={n=>resize("inspectorSize",n)}/></div>}
    {panels.right&&<aside className={"right dock-panel size-"+panels.inspectorSize}><h3 className="dock-title"><Icon name="inspector"/> Inspector<span className="grow"/><button aria-label="Hide inspector" onClick={()=>toggle("right")}><Icon name="close"/></button></h3><div className="inspector-tabs">{(["properties","layout","bindings","events"] as const).map(t=><button key={t} className={tab===t?"active":""} onClick={()=>setTab(t)}>{t}</button>)}</div>{chosen&&<div className="inspect-card"><Icon name="layers"/><div><strong>{label(chosen)}</strong><small>{chosen.element.kind??chosen.element.type}</small></div></div>}{tab==="properties"?<>{screen&&<EditField key={generation.current+":"+screen.id+":"+snapshot?.revision} label="Screen name" value={screen.name} disabled={busy||pendingRecovery} normalize={v=>v.trim()} commit={name=>execute({op:"renameScreen",screen:screen.id,name},"Rename screen")}/>}{chosen?<><dl><dt>ID</dt><dd>{chosen.id}</dd><dt>Type</dt><dd>{chosen.element.kind??chosen.element.type}</dd></dl>{chosenContract&&snapshot&&<ContractEditor key={generation.current+":"+chosen.id+":"+snapshot.revision} contract={chosenContract} project={snapshot.project} disabled={!ready} commit={contract=>execute({op:"setV2ComponentContract",node:chosen.id,contract},"Edit typed component")}/>}{(chosen.element.kind==="text"||chosen.element.kind==="button")&&<EditField key={generation.current+":"+chosen.id+":"+snapshot?.revision} label="Text" value={typeof chosen.properties?.text==="string"?chosen.properties.text:""} disabled={busy||pendingRecovery||typeof chosen.properties?.text==="object"} commit={value=>execute({op:"setProperty",node:chosen.id,name:"text",value},"Edit text")}/>}</>:<p>Select a layer.</p>}</>:tab==="layout"?chosen?<><h3>SIZE / FLOW</h3>{(isFlow(chosen.element)||chosen.element.type==="component")?<div key={generation.current+":"+chosen.id+":"+snapshot?.revision}><SizeEditor axis="Width" size={chosen.layout?.width} disabled={!ready} commit={width=>execute({op:"setLayout",node:chosen.id,layout:{...chosen.layout,width}},"Set width")}/><SizeEditor axis="Height" size={chosen.layout?.height} disabled={!ready} commit={height=>execute({op:"setLayout",node:chosen.id,layout:{...chosen.layout,height}},"Set height")}/>{isFlow(chosen.element)&&<FlowEditor element={chosen.element} disabled={!ready} commit={element=>execute({op:"setElement",node:chosen.id,element},"Set flow arrangement")}/>}</div>:<p>Layout editing is not available for this element.</p>}{previewLimit(chosen).map(l=><p className="layout-warning" key={l}>{l}</p>)}</>:<p>Select a layer to edit its layout.</p>:<>{chosen&&snapshot&&tab==="bindings"&&<><h3>V2 DECLARATIONS</h3>{snapshot.project.v2?.bindings?.expressions?.filter(x=>x.node===chosen.id).map(x=><div className="v2-declaration" key={x.property}><strong>{x.property}</strong><code>{x.expression}</code><small>Contract {x.contractVersion} · not evaluated</small></div>)}{chosenContract&&(chosenContract.kind==="textInput"||chosenContract.kind==="dropDown")&&<p>Tag: {chosenContract.tag} · edit in Properties.</p>}{snapshot.project.v2?.industrial?.nodes?.filter(x=>x.node===chosen.id).map(x=><p key={x.requiredPermission}>Required permission: {x.requiredPermission}</p>)}</>}{chosen&&snapshot&&tab==="events"&&snapshot.project.v2?.components?.handlers?.filter(x=>x.origin.node===chosen.id).map(x=><div className="v2-declaration" key={x.event}><strong>{x.event}</strong><small>{x.actions.map(a=>a.type).join(" → ")}</small></div>)}<p>Full {tab} editing is planned. Existing declarations are preserved.</p></>}</aside>}
    </section>
    {panels.diagnostics&&<Splitter label="Resize diagnostics" size={panels.diagnosticsSize} vertical={false} direction={-1} change={n=>resize("diagnosticsSize",n)}/>}
    {panels.diagnostics&&<section className={"diagnostics dock-diagnostics size-"+panels.diagnosticsSize}><div className="output-tabs" aria-label="Engineering output"><button aria-pressed={outputView==="simulation"} onClick={()=>setOutputView("simulation")}><Icon name="tag"/> Simulated tags</button><button disabled title="Engineering search UI planned"><Icon name="search"/> Engineering search</button><button disabled title="Alarm monitoring UI planned"><Icon name="bell"/> Alarms</button><button disabled title="Event log UI planned"><Icon name="terminal"/> Event log</button><button aria-pressed={outputView==="diagnostics"} onClick={()=>setOutputView("diagnostics")}><Icon name="terminal"/> Diagnostics <span>{snapshot?.status.errorCount??0} errors · {snapshot?.status.warningCount??0} warnings</span></button><button aria-pressed={outputView==="status"} onClick={()=>setOutputView("status")}>Status</button></div>{outputView==="simulation"?simulationOutput:outputView==="status"?<p>{status}</p>:<>{snapshot?.diagnostics.length?snapshot.diagnostics.map((d,i)=><p key={i}>{d.severity} · {d.code} · {d.path} · {d.message}</p>):<p>No diagnostics reported.</p>}</>}</section>}
    <footer><span className="no-plc">{reviewRequired?"● Import review required · production/live blocked":"● No live PLC"}</span><span role="status">{status}</span><span className="grow"/><span>{(snapshot?.status.dirty?"Unsaved changes":"No unsaved edits")+" · "+autosaveMessage}</span><span title="Planned runtime targets">Windows · Linux · Android</span></footer>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Studio/>);
