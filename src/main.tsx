import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import "./style.css";

type Component = { id: number; kind: "button" | "text" | "gauge"; label: string };
type Snapshot = { version: string; components: Component[]; selected: number | null };
type Command = { type: "snapshot" } | { type: "insert"; kind: Component["kind"] } | { type: "select"; id: number };
const menus: Record<string,string[]> = {File:["New project","Open project","Save project"],Edit:["Undo","Redo"],View:["Workspace","Zoom"],Insert:["Button","Text","Gauge"],Arrange:["Align left","Distribute"],Project:["Validate project"],PLC:["Connections","Tags"],Simulation:["Start simulation"],Runtime:["Export","Publish"],Tools:["Preferences"],Help:["About"]};
function Studio(){
 const [snapshot,setSnapshot]=useState<Snapshot|null>(null);
 const [status,setStatus]=useState("Connecting to engineering core…");
 const [menu,setMenu]=useState<string|null>(null);
 const [busy,setBusy]=useState(false);
 async function command(command:Command){setBusy(true);try{const result=await invoke<Snapshot>("engineering_command",{command});setSnapshot(result);setStatus("Rust engineering core ready · in-memory demo");}catch(e){setStatus("Command failed: "+String(e));}finally{setBusy(false)}}
 useEffect(()=>{void command({type:"snapshot"})},[]);
 const chosen=snapshot?.components.find(c=>c.id===snapshot.selected);
 function action(name:string){setMenu(null);const kind=name.toLowerCase();if(kind==="button"||kind==="text"||kind==="gauge"){void command({type:"insert",kind});}else if(name==="About"){setStatus("HMI Studio "+(snapshot?.version??"0.1.0")+" · development shell");}else{setStatus(name+" — not implemented in this milestone");}}
 return <main className="studio" onClick={()=>setMenu(null)}>
 <header><strong>◈ HMI STUDIO</strong><span>Untitled project — development shell</span><small>v{snapshot?.version??"0.1.0"}</small></header>
 <nav aria-label="Main menu" onClick={e=>e.stopPropagation()}>{Object.entries(menus).map(([name,items])=><div className="menu" key={name}><button aria-expanded={menu===name} onClick={()=>setMenu(menu===name?null:name)}>{name}</button>{menu===name&&<div className="dropdown">{items.map(item=><button disabled={busy} key={item} onClick={()=>action(item)}>{item}{name!=="Insert"&&item!=="About"?" · planned":""}</button>)}</div>}</div>)}</nav>
 <div className="toolbar"><button onClick={()=>action("New project")}>＋ New</button><button onClick={()=>action("Save project")}>▣ Save</button><button onClick={()=>action("Undo")}>↶</button><button onClick={()=>action("Redo")}>↷</button><i/>{["Button","Text","Gauge"].map(n=><button key={n} disabled={busy||!snapshot} onClick={()=>action(n)}>＋ {n}</button>)}<i/><button onClick={()=>action("Align left")}>Align</button><span className="grow"/><button onClick={()=>action("Start simulation")}>▷ Preview</button><button className="accent" onClick={()=>action("Publish")}>↑ Publish</button></div>
 <section className="workspace">
 <aside className="left"><h3>PROJECT EXPLORER</h3><div className="tree">▾ Untitled project<br/>　▾ Screens<br/><span className="active">　　 Overview</span><br/>　▸ PLC connections<br/>　▸ Tags<br/>　▸ Alarms<br/>　▸ Recipes<br/>　▸ Assets</div><h3>COMPONENTS / LAYERS</h3>{snapshot?.components.map(c=><button className={"layer "+(chosen?.id===c.id?"active":"")} disabled={busy} key={c.id} onClick={()=>void command({type:"select",id:c.id})}>{c.kind} · {c.label}</button>)}</aside>
 <section className="designer"><div className="tab">Overview.hmi <span>●</span></div><div className="viewport"><div className="screen"><div className="screen-title">PRODUCTION LINE — OVERVIEW</div><div className="objects">{snapshot?.components.map(c=><button key={c.id} className={"object "+c.kind+(chosen?.id===c.id?" selected":"")} disabled={busy} onClick={()=>void command({type:"select",id:c.id})}>{c.kind==="gauge"?<><small>{c.label}</small><strong>32.5</strong><small>Demo value</small></>:c.label}</button>)}</div></div></div></section>
 <aside className="right"><h3>INSPECTOR</h3><div className="inspector-tabs">Properties　 Style　 Events</div>{chosen?<dl><dt>ID</dt><dd>{chosen.id}</dd><dt>Type</dt><dd>{chosen.kind}</dd><dt>Label</dt><dd>{chosen.label}</dd><dt>Layout</dt><dd>Demo flex layout</dd><dt>Binding</dt><dd>Not configured</dd></dl>:<p>Select a component.</p>}<p className="muted">Property editing is planned.</p></aside>
 </section><section className="diagnostics"><h3>DIAGNOSTICS　 <span>Search results　 Tag monitor　 Build output (planned)</span></h3><p role="status">{status}</p></section><footer>Development shell · No live PLC connection <span>100%　|　Overview</span></footer></main>
}
createRoot(document.getElementById("root")!).render(<Studio/>);
