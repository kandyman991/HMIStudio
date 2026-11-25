// Presentation only. Never reserialize the full project or its integer
// enumeration values through JavaScript. Rust owns authoritative contracts.
import type {Layout} from "./layout-preview";
export type Point={x:number;y:number};
export type LocalizedLabel=string|{textKey:string};
export type Contract=
 | {kind:"line";start:Point;end:Point;strokeColor:string;strokeWidth:number}
 | {kind:"arrow";start:Point;end:Point;strokeColor:string;strokeWidth:number;headLength:number}
 | {kind:"polygon";points:Point[];fill:string;strokeColor:string;strokeWidth:number}
 | {kind:"textInput";tag:string;maxLength:number;label?:LocalizedLabel}
 | {kind:"dropDown";tag:string;enumeration:string;label?:LocalizedLabel};
export type Kind=Contract["kind"];
export type ContractEntry={id:string;contract:Contract};
export type TagChoice={id:string;name:string;dataType:unknown;access?:string};
export type EnumChoice={id:string;name:string;dataType:unknown;entries:{label:LocalizedLabel}[]};
export type V2Project={tags?:TagChoice[];localization?:{defaultLocale:string;strings?:Record<string,Record<string,string>>};v2?:{
 industrial?:{screens?:{screen:string;requiredPermission:string}[];nodes?:{node:string;requiredPermission:string}[]};
 bindings?:{enumerations?:EnumChoice[];expressions?:{node:string;property:string;expression:string;contractVersion:number}[]};
 components?:{components?:ContractEntry[];handlers?:{origin:{node?:string;screen?:string};event:string;actions:{type:string}[]}[];definitions?:{definition:string;classification:{kind:string;symbol?:string}}[]};
}};
export const KINDS:Kind[]=["line","arrow","polygon","textInput","dropDown"];
export const kindLabel:Record<Kind,string>={line:"Line",arrow:"Arrow",polygon:"Polygon",textInput:"Text input",dropDown:"Dropdown"};
export function contractFor(p:V2Project|undefined,id:string):Contract|undefined{return p?.v2?.components?.components?.find(x=>x.id===id)?.contract;}
export function writableStrings(p:V2Project):TagChoice[]{return (p.tags??[]).filter(t=>t.dataType==="string"&&t.access==="readWrite");}
export function enumPairs(p:V2Project):{tag:TagChoice;enumeration:EnumChoice}[]{
 return (p.tags??[]).filter(t=>t.access==="readWrite").flatMap(tag=>(p.v2?.bindings?.enumerations??[]).filter(e=>typeof e.dataType==="string"&&e.dataType===tag.dataType).map(enumeration=>({tag,enumeration})));
}
export function defaultContract(kind:Kind,p:V2Project):Contract|undefined{
 const start={x:8,y:8},end={x:108,y:48},strokeColor="#D0E1EF",strokeWidth=2;
 switch(kind){
 case "line":return {kind,start,end,strokeColor,strokeWidth};
 case "arrow":return {kind,start,end,strokeColor,strokeWidth,headLength:12};
 case "polygon":return {kind,points:[start,{x:108,y:8},{x:58,y:68}],fill:"#426783",strokeColor,strokeWidth};
 case "textInput":{const tag=writableStrings(p)[0];return tag?{kind,tag:tag.id,maxLength:80}:undefined;}
 case "dropDown":{const pair=enumPairs(p)[0];return pair?{kind,tag:pair.tag.id,enumeration:pair.enumeration.id}:undefined;}
 }
}
export function defaultLayout(kind:Kind):Layout{return {width:{mode:"fixed",value:kind==="textInput"||kind==="dropDown"?160:120},height:{mode:"fixed",value:kind==="textInput"||kind==="dropDown"?48:80}};}
export function localizedLabel(label:LocalizedLabel|undefined,p:V2Project):string{
 if(typeof label==="string")return label;
 if(label){return p.localization?.strings?.[label.textKey]?.[p.localization.defaultLocale]??("["+label.textKey+"]");}
 return "";
}
// Strict coordinate input, including negative values; no blank-to-zero coercion.
export function parseCoordinate(s:string):number|undefined{
 if(!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(s.trim()))return;
 const n=Number(s);return Number.isFinite(n)?n:undefined;
}
export function parsePoints(s:string):Point[]|undefined{
 const rows=s.trim().split(/\r?\n/);if(rows.length<3||rows.length>1024)return;
 const out:Point[]=[];
 for(const row of rows){const parts=row.split(",");if(parts.length!==2)return;const x=parseCoordinate(parts[0]),y=parseCoordinate(parts[1]);if(x===undefined||y===undefined)return;out.push({x,y});}
 return out;
}
export type Geometry={points:Point[];head?:Point[];viewBox:string};
export function geometry(c:Contract):Geometry|undefined{
 if(c.kind==="textInput"||c.kind==="dropDown")return;
 const points=c.kind==="polygon"?c.points:[c.start,c.end];
 let head:Point[]|undefined;
 if(c.kind==="arrow"){
   const dx=c.end.x-c.start.x,dy=c.end.y-c.start.y,len=Math.hypot(dx,dy);
   if(!(len>0&&Number.isFinite(len)))return;
   const ux=dx/len,uy=dy/len,h=c.headLength,w=h*.45;
   head=[c.end,{x:c.end.x-ux*h-uy*w,y:c.end.y-uy*h+ux*w},{x:c.end.x-ux*h+uy*w,y:c.end.y-uy*h-ux*w}];
 }
 const all=[...points,...(head??[])];
 if(!all.length||all.some(p=>!Number.isFinite(p.x)||!Number.isFinite(p.y)))return;
 const pad=Math.max(c.strokeWidth*2,1),minX=Math.min(...all.map(p=>p.x))-pad,minY=Math.min(...all.map(p=>p.y))-pad;
 const width=Math.max(...all.map(p=>p.x))-minX+pad,height=Math.max(...all.map(p=>p.y))-minY+pad;
 if(![minX,minY,width,height].every(Number.isFinite)||width<=0||height<=0)return;
 return {points,head,viewBox:[minX,minY,width,height].join(" ")};
}
