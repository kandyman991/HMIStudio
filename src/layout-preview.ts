// Deterministic, presentation-only CSS mapping for base semantic flow layouts.
// Rust owns validation and editing; this is not a Compose layout solver.
import type {CSSProperties} from "react";
export type Size = {mode:"auto"}|{mode:"fixed";value:number}|{mode:"fill";fraction?:number};
export type Cross = "start"|"center"|"end";
export type Arrangement = "start"|"center"|"end"|"spaceBetween"|"spaceAround"|"spaceEvenly";
export type Element = {type:string;kind?:string;spacing?:number;arrangement?:Arrangement;alignment?:Cross;wrap?:boolean;[key:string]:unknown};
export type Layout = {width?:Size;height?:Size;minWidth?:number;maxWidth?:number;minHeight?:number;maxHeight?:number;aspectRatio?:number;padding?:{top?:number;end?:number;bottom?:number;start?:number};weight?:number;crossAlign?:Cross;visibility?:"visible"|"hidden"|"collapsed";clip?:boolean;zIndex?:number;[key:string]:unknown};
export type PreviewNode = {element:Element;layout?:Layout;responsive?:unknown[];bindings?:unknown[];style?:string;events?:unknown[];properties?:Record<string,unknown>};
const number=(n:unknown):n is number=>typeof n==="number"&&Number.isFinite(n)&&n>=0;
const positive=(n:unknown):n is number=>number(n)&&n>0;
const cross=(a:Cross|undefined)=>a==="center"?"center":a==="end"?"flex-end":"flex-start";
const arrangement=(a:Arrangement|undefined)=>a==="spaceBetween"?"space-between":a==="spaceAround"?"space-around":a==="spaceEvenly"?"space-evenly":cross(a as Cross|undefined);
export const isFlow=(e:Element)=>e.type==="row"||e.type==="column";
export function sizeCss(s:Size|undefined):string|number{
  if(s?.mode==="fixed"&&number(s.value))return s.value;
  if(s?.mode==="fill"){const f=s.fraction??1;if(positive(f)&&f<=1)return (f*100)+"%";}
  return "auto";
}
export function nodeStyle(n:PreviewNode,parent?:Element):CSSProperties{
  const l=n.layout??{},e=n.element;
  const st:CSSProperties={boxSizing:"border-box",width:sizeCss(l.width),height:sizeCss(l.height),
    minWidth:number(l.minWidth)?l.minWidth:0,minHeight:number(l.minHeight)?l.minHeight:0,
    maxWidth:number(l.maxWidth)?l.maxWidth:undefined,maxHeight:number(l.maxHeight)?l.maxHeight:undefined,
    aspectRatio:positive(l.aspectRatio)?l.aspectRatio:undefined,
    paddingTop:number(l.padding?.top)?l.padding.top:0,paddingRight:number(l.padding?.end)?l.padding.end:0,
    paddingBottom:number(l.padding?.bottom)?l.padding.bottom:0,paddingLeft:number(l.padding?.start)?l.padding.start:0,
    flexShrink:0,position:"relative",overflow:l.clip?"hidden":"visible",
    visibility:l.visibility==="hidden"?"hidden":undefined,pointerEvents:l.visibility==="hidden"?"none":undefined,zIndex:typeof l.zIndex==="number"&&Number.isSafeInteger(l.zIndex)?l.zIndex:undefined};
  if(l.visibility==="collapsed"){st.display="none";return st;}
  if(parent&&isFlow(parent)){
    if(l.crossAlign)st.alignSelf=cross(l.crossAlign);
    if(positive(l.weight)){st.flexGrow=l.weight;st.flexBasis=0;st.flexShrink=1;}
  }
  if(parent?.type==="absolute"){
    const pos=l.position as {x?:unknown;y?:unknown}|undefined;
    st.position="absolute";
    st.left=typeof pos?.x==="number"&&Number.isFinite(pos.x)?pos.x:0;
    st.top=typeof pos?.y==="number"&&Number.isFinite(pos.y)?pos.y:0;
  }
  if(e.type==="box"){st.display="grid";}
  if(parent?.type==="box"){st.gridArea="1 / 1";}
  if(isFlow(e)){
    st.display="flex";st.flexDirection=e.type==="row"?"row":"column";
    st.flexWrap=e.wrap?"wrap":"nowrap";st.alignContent="flex-start";st.alignItems=cross(e.alignment);
    st.justifyContent=arrangement(e.arrangement);
    st.gap=typeof e.arrangement==="string"&&e.arrangement.startsWith("space")?0:number(e.spacing)?e.spacing:0;
  }
  return st;
}
export function previewLimit(n:PreviewNode):string[]{
  const issues:string[]=[];
  if(!isFlow(n.element)&&n.element.type!=="component"&&n.element.type!=="absolute")issues.push("Unsupported container / instance: "+n.element.type);
  if(n.element.type==="component"&&!["text","button","gauge","line","arrow","polygon","textInput","dropDown"].includes(n.element.kind??""))issues.push("Static placeholder: "+(n.element.kind??"component"));
  if(n.responsive?.length)issues.push("Responsive overrides not applied");
  if(n.bindings?.length)issues.push("Live bindings not evaluated");
  if(n.style)issues.push("Named style not applied");
  if(n.events?.length)issues.push("Events not executed");
  if(n.properties?.text!==undefined&&typeof n.properties.text!=="string")issues.push("Localized or nonliteral text not resolved");
  return issues;
}
export function parseFixed(value:string):number|undefined{
  if(!/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value.trim()))return undefined;
  const n=Number(value);
  return number(n)?n:undefined;
}

// Presentation only. Reserve space for the two rulers and the editor chrome;
// the logical artboard stays 800 × 480 dp regardless of display zoom.
export const PREVIEW_WIDTH=826, PREVIEW_HEIGHT=502;
export function fitPreview(width:number,height:number):number{
  if(!positive(width)||!positive(height))return 1;
  return Math.max(.25,Math.min(1,Math.floor(100*Math.min(Math.max(0,width-32)/PREVIEW_WIDTH,Math.max(0,height-94)/PREVIEW_HEIGHT))/100));
}
export function changeZoom(current:number,delta:number):number{
  return Math.max(.25,Math.min(2,Math.round((current+delta)*100)/100));
}
