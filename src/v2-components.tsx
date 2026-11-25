import {useState} from "react";
import type {Contract,V2Project,Point,LocalizedLabel} from "./v2";
import {geometry,localizedLabel,kindLabel,writableStrings,enumPairs,parseCoordinate,parsePoints} from "./v2";

export function ContractPreview({contract:c,project:p}:{contract:Contract;project:V2Project}){
 if(c.kind==="textInput"||c.kind==="dropDown"){
   const name=p.tags?.find(t=>t.id===c.tag)?.name??c.tag;
   const options=c.kind==="dropDown"?p.v2?.bindings?.enumerations?.find(e=>e.id===c.enumeration)?.entries:undefined;
   return <span className="v2-input-preview"><small>{localizedLabel(c.label,p)||kindLabel[c.kind]}</small><span className="v2-input-field">{c.kind==="dropDown"?(options?.length?localizedLabel(options[0].label,p)+" ▾":"[No options]"):"Enter text…"}</span><small>{name} · design only</small></span>;
 }
 const g=geometry(c);if(!g)return <span>Invalid geometry</span>;
 const list=(points:Point[])=>points.map(pt=>pt.x+","+pt.y).join(" ");
 return <svg className="v2-shape" viewBox={g.viewBox} aria-label={kindLabel[c.kind]} preserveAspectRatio="xMidYMid meet">
 {c.kind==="polygon"?<polygon points={list(g.points)} fill={c.fill} stroke={c.strokeColor} strokeWidth={c.strokeWidth}/>:<line x1={c.start.x} y1={c.start.y} x2={c.end.x} y2={c.end.y} stroke={c.strokeColor} strokeWidth={c.strokeWidth} strokeLinecap="round"/>}
 {g.head&&<polygon points={list(g.head)} fill={c.strokeColor}/>}
 </svg>;
}
export function ContractEditor({contract:c,project:p,disabled,commit}:{contract:Contract;project:V2Project;disabled:boolean;commit:(c:Contract)=>void}){
 // Each field submits a typed single-contract edit. No other project
 // declarations or numeric enumeration values travel back through JS.
 const [draft,setDraft]=useState<Contract>(c);
 const [raw,setRaw]=useState<Record<string,string>>({});
 const [error,setError]=useState("");
 const field=(name:string,value:number,apply:(n:number)=>void)=><label key={name}>{name}<input disabled={disabled} value={raw[name]??String(value)} inputMode="decimal" onChange={e=>{const s=e.currentTarget.value;setRaw(old=>({...old,[name]:s}));const n=parseCoordinate(s);if(n!==undefined)apply(n);}}/></label>;
 const color=(name:string,value:string,apply:(v:string)=>void)=><label>{name}<input disabled={disabled} value={value} onChange={e=>apply(e.currentTarget.value)}/></label>;
 const point=(name:string,v:Point,apply:(v:Point)=>void)=><>{field(name+" X",v.x,x=>apply({...v,x}))}{field(name+" Y",v.y,y=>apply({...v,y}))}</>;
 const labelEditor=(label:LocalizedLabel|undefined,apply:(v:LocalizedLabel|undefined)=>void)=><><label>Label type<select disabled={disabled} value={label===undefined?"none":typeof label==="string"?"literal":"key"} onChange={e=>apply(e.currentTarget.value==="none"?undefined:e.currentTarget.value==="literal"?"":{textKey:""})}><option value="none">Default</option><option value="literal">Literal</option><option value="key">Localization key</option></select></label>{label!==undefined&&(typeof label==="string"?<label>Label<input disabled={disabled} value={label} onChange={e=>apply(e.currentTarget.value)}/></label>:<label>Text key<select disabled={disabled} value={label.textKey} onChange={e=>apply({textKey:e.currentTarget.value})}><option value="">Select key</option>{Object.keys(p.localization?.strings??{}).map(k=><option key={k}>{k}</option>)}</select></label>)}</>;
 const pairs=enumPairs(p);
 return <form className="layout-field v2-contract-editor" onSubmit={e=>{e.preventDefault();if(disabled)return;
   if(Object.entries(raw).some(([k,v])=>k==="Points"?parsePoints(v)===undefined:parseCoordinate(v)===undefined)){setError("Enter valid finite coordinates or numeric values.");return;}
   setError("");commit(draft);
 }}>
 <h3>{kindLabel[c.kind]} properties</h3>
 {draft.kind==="line"||draft.kind==="arrow"?<>
 {point("Start",draft.start,start=>setDraft({...draft,start}))}{point("End",draft.end,end=>setDraft({...draft,end}))}
 {color("Stroke",draft.strokeColor,strokeColor=>setDraft({...draft,strokeColor}))}{field("Width",draft.strokeWidth,strokeWidth=>setDraft({...draft,strokeWidth}))}
 {draft.kind==="arrow"&&field("Head length",draft.headLength,headLength=>setDraft({...draft,headLength}))}
 </>:draft.kind==="polygon"?<>
 <label>Points (x,y per line)<textarea disabled={disabled} rows={5} value={raw.Points??draft.points.map(pt=>pt.x+","+pt.y).join("\n")} onChange={e=>{const s=e.currentTarget.value;setRaw(old=>({...old,Points:s}));const points=parsePoints(s);if(points)setDraft({...draft,points});}}/></label>
 {color("Fill",draft.fill,fill=>setDraft({...draft,fill}))}{color("Stroke",draft.strokeColor,strokeColor=>setDraft({...draft,strokeColor}))}{field("Width",draft.strokeWidth,strokeWidth=>setDraft({...draft,strokeWidth}))}
 </>:draft.kind==="textInput"?<>
 <label>Tag<select disabled={disabled} value={draft.tag} onChange={e=>setDraft({...draft,tag:e.currentTarget.value})}>{writableStrings(p).map(t=><option key={t.id} value={t.id}>{t.name}</option>)}</select></label>
 {field("Max length",draft.maxLength,maxLength=>setDraft({...draft,maxLength}))}
 {labelEditor(draft.label,label=>setDraft({...draft,label}))}
 </>:<>
 <label>Tag / enum<select disabled={disabled} value={JSON.stringify([draft.tag,draft.enumeration])} onChange={e=>{const pair=pairs.find(x=>JSON.stringify([x.tag.id,x.enumeration.id])===e.currentTarget.value);if(pair)setDraft({...draft,tag:pair.tag.id,enumeration:pair.enumeration.id});}}>{pairs.map(x=><option key={JSON.stringify([x.tag.id,x.enumeration.id])} value={JSON.stringify([x.tag.id,x.enumeration.id])}>{x.tag.name} / {x.enumeration.name}</option>)}</select></label>
 {labelEditor(draft.label,label=>setDraft({...draft,label}))}
 </>}
 {error&&<p role="alert">{error}</p>}<button disabled={disabled||JSON.stringify(draft)===JSON.stringify(c)} type="submit">Apply typed properties</button>
 <small>Rust validates the edit. No device I/O.</small>
 </form>;
}
