import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";

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
type Panels = { left: boolean; right: boolean; diagnostics: boolean };
const defaults: Panels = {left:true,right:true,diagnostics:true};
function readPanels(): Panels {
  try { const saved = JSON.parse(localStorage.getItem("studio.workspace.v1") ?? "null"); if(saved && Object.keys(defaults).every(k => typeof saved[k] === "boolean")) return {left:saved.left,right:saved.right,diagnostics:saved.diagnostics}; } catch { /* storage unavailable */ }
  return defaults;
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
  const [screenId,setScreenId]=useState<string|null>(null);
  const [selected,setSelected]=useState<string|null>(null);
  const [panels,setPanels]=useState<Panels>(readPanels);
  const [tab,setTab]=useState<"properties"|"bindings"|"events">("properties");
  const inFlight=useRef(false);
  const screen=snapshot?.project.screens.find(s=>s.id===screenId)??snapshot?.project.screens[0];
  const chosen=screen?findNode(screen.root,selected):undefined;
  const send=useCallback(async(request:Request)=>{
    if(inFlight.current){setStatus("Another engineering command is pending; retry after it finishes.");return;}
    inFlight.current=true;setBusy(true);
    try{
      const reply=await invoke<Reply>("engineering_request",{request});
      setSnapshot(reply.snapshot);
      if(reply.response.response==="error")setStatus(reply.response.error?.message??"Command rejected");
      else {setStatus("Engineering core ready · revision "+reply.snapshot.revision);const ids=reply.response.outcome?.allocatedIds;if(ids?.["0:screen"]){setScreenId(ids["0:screen"]);setSelected(null);}if(ids?.["0:node:pending"])setSelected(ids["0:node:pending"]);}
    }catch(e){setStatus("Command failed: "+String(e));}
    finally{inFlight.current=false;setBusy(false);}
  },[]);
  useEffect(()=>{void send({request:"snapshot"});},[send]);
  useEffect(()=>{try{localStorage.setItem("studio.workspace.v1",JSON.stringify(panels));}catch{/* only presentation preferences */}},[panels]);
  const execute=(command:Command,label:string)=>{if(snapshot)void send({request:"execute",transaction:{commands:[command],expectedRevision:snapshot.revision,label}})};
  const history=useCallback((request:"undo"|"redo")=>{if(snapshot && (request==="undo"?snapshot.status.canUndo:snapshot.status.canRedo))void send({request,expectedRevision:snapshot.revision});},[snapshot,send]);
  useEffect(()=>{function keys(e:KeyboardEvent){if(e.key==="Escape")setMenu(null);if((e.ctrlKey||e.metaKey)&&!e.altKey&&!(e.target instanceof HTMLInputElement)&&!(e.target instanceof HTMLTextAreaElement)){if(e.key.toLowerCase()==="z"){e.preventDefault();history(e.shiftKey?"redo":"undo");}else if(e.key.toLowerCase()==="y"){e.preventDefault();history("redo");}}}window.addEventListener("keydown",keys);return()=>window.removeEventListener("keydown",keys);},[history]);
  const toggle=(key:keyof Panels)=>setPanels(p=>({...p,[key]:!p[key]}));
  const insert=(kind:"button"|"text"|"gauge")=>{if(screen)execute({op:"insertNode",parent:screen.root.id,node:{id:"pending",element:{type:"component",kind},properties:kind==="gauge"?{value:0,min:0,max:100}:{text:kind==="button"?"Button":"Text"}},assignNewIds:true},"Insert "+kind);};
  function action(name:string){setMenu(null);if(inFlight.current){setStatus("Another engineering command is pending; retry after it finishes.");return;}if(name==="New screen")execute({op:"createScreen",name:"Screen "+((snapshot?.project.screens.length??0)+1)},"New screen");else if(name==="Undo"||name==="Redo")history(name==="Undo"?"undo":"redo");else if(name==="Button"||name==="Text"||name==="Gauge")insert(name.toLowerCase() as "button"|"text"|"gauge");else if(name==="Delete selection"&&chosen&&screen&&chosen.id!==screen.root.id)execute({op:"removeNode",node:chosen.id},"Delete selection");else if(name==="Project explorer")toggle("left");else if(name==="Inspector")toggle("right");else if(name==="Diagnostics")toggle("diagnostics");else if(name==="Reset workspace")setPanels(defaults);else if(name==="Validate project")void send({request:"validate"});else if(name==="Exit (discard session)"){if(window.confirm("Exit and discard this in-memory session? Project saving is not available in this slice."))void invoke("discard_and_exit").catch(e=>setStatus(String(e)));}else setStatus(name+" — planned");}
  const menus:Record<string,string[]>={File:["New screen","Open project (planned)","Save project (planned)","Exit (discard session)"],Edit:["Undo","Redo","Delete selection"],View:["Project explorer","Inspector","Diagnostics","Reset workspace"],Insert:["Button","Text","Gauge"],Arrange:["Align (planned)"],Project:["Validate project"],PLC:["Connections (planned)"],Simulation:["Preview (planned)"],Runtime:["Export (planned)","Publish (planned)"],Tools:["Preferences (planned)"],Help:["About (planned)"]};
  const disabled=(name:string)=>name.includes("(planned)")||busy||(!snapshot&&name!=="Reset workspace")||(name==="Undo"&&!snapshot?.status.canUndo)||(name==="Redo"&&!snapshot?.status.canRedo)||(name==="Delete selection"&&(!chosen||chosen.id===screen?.root.id));
  return <main className="studio" onClick={()=>setMenu(null)}>
    <header><strong>◈ HMI STUDIO</strong><span>{snapshot?.project.metadata.name??"Loading"}{snapshot?.status.dirty?" *":""} · Untitled session</span><small>RUST-02A</small></header>
    <nav aria-label="Main menu" onClick={e=>e.stopPropagation()}>{Object.entries(menus).map(([name,items])=><div className="menu" key={name}><button aria-expanded={menu===name} onClick={()=>setMenu(menu===name?null:name)}>{name}</button>{menu===name&&<div className="dropdown">{items.map(item=><button disabled={disabled(item)} key={item} onClick={()=>action(item)}>{item}</button>)}</div>}</div>)}</nav>
    <div className="toolbar">{["New screen","Undo","Redo","Button","Text","Gauge","Delete selection","Validate project"].map(name=><button key={name} disabled={disabled(name)} onClick={()=>action(name)}>{name}</button>)}<span className="grow"/><button onClick={()=>toggle("left")}>Explorer</button><button onClick={()=>toggle("right")}>Inspector</button><button onClick={()=>toggle("diagnostics")}>Diagnostics</button></div>
    <section className={"workspace "+(!panels.left?"hide-left ":"")+(!panels.right?"hide-right":"")}>
    {panels.left&&<aside className="left"><h3>PROJECT EXPLORER</h3><div className="tree">{snapshot?.project.screens.map(s=><button key={s.id} className={"layer "+(screen?.id===s.id?"active":"")} disabled={busy} onClick={()=>{if(inFlight.current)return;setScreenId(s.id);setSelected(null);}}>{s.name}</button>)}</div><h3>COMPONENTS / LAYERS</h3>{screen&&<Tree node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/>}</aside>}
    <section className="designer"><div className="screen-tabs">{snapshot?.project.screens.map(s=><button key={s.id} className={screen?.id===s.id?"active":""} disabled={busy} onClick={()=>{if(inFlight.current)return;setScreenId(s.id);setSelected(null);}}>{s.name}</button>)}</div><div className="viewport">{screen&&<div className="screen"><div className="screen-title">{screen.name} · structural preview</div><Preview node={screen.root} selected={selected} select={id=>{if(!inFlight.current)setSelected(id);}}/></div>}</div></section>
    {panels.right&&<aside className="right"><h3>INSPECTOR</h3><div className="inspector-tabs">{(["properties","bindings","events"] as const).map(t=><button key={t} className={tab===t?"active":""} onClick={()=>setTab(t)}>{t}</button>)}</div>{tab==="properties"?<>{screen&&<EditField key={screen.id+":"+snapshot?.revision} label="Screen name" value={screen.name} disabled={busy} normalize={v=>v.trim()} commit={name=>execute({op:"renameScreen",screen:screen.id,name},"Rename screen")}/>}{chosen?<><dl><dt>ID</dt><dd>{chosen.id}</dd><dt>Type</dt><dd>{chosen.element.kind??chosen.element.type}</dd></dl>{(chosen.element.kind==="text"||chosen.element.kind==="button")&&<EditField key={chosen.id+":"+snapshot?.revision} label="Text" value={typeof chosen.properties?.text==="string"?chosen.properties.text:""} disabled={busy||typeof chosen.properties?.text==="object"} commit={value=>execute({op:"setProperty",node:chosen.id,name:"text",value},"Edit text")}/>}</>:<p>Select a layer.</p>}</>:<p>Editing {tab} is planned.</p>}</aside>}
    </section>
    {panels.diagnostics&&<section className="diagnostics"><h3>DIAGNOSTICS <span>{snapshot?.status.errorCount??0} errors · {snapshot?.status.warningCount??0} warnings</span></h3><p role="status">{status}</p>{snapshot?.diagnostics.map((d,i)=><p key={i}>{d.severity} · {d.code} · {d.path} · {d.message}</p>)}</section>}
    <footer><span role="status">{status}</span><span>{snapshot?.status.dirty?"Unsaved · File > Exit (discard session)":"In-memory project"} · No live PLC</span></footer>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Studio/>);
