// Pure selection rules for the first semantic designer slice.
// Grid, absolute, scroll and box placement have their own semantics and are
// intentionally not inferred here. The Rust engine remains authoritative.
export type LayoutNode = {
  id: string;
  element: { type: string; wrap?: boolean };
  layout?: { visibility?: string; width?: {mode:string}; height?: {mode:string}; weight?: number; aspectRatio?: number; minWidth?: number; maxWidth?: number; minHeight?: number; maxHeight?: number };
  responsive?: unknown[];
  children?: LayoutNode[];
};
function isFlow(node: LayoutNode): boolean {
  return node.element.type === "row" || node.element.type === "column";
}
function locate(root: LayoutNode, id: string, parent?: LayoutNode): {node:LayoutNode;parent?:LayoutNode} | undefined {
  if(root.id===id)return {node:root,parent};
  for(const child of root.children??[]){
    const match=locate(child,id,root);
    if(match)return match;
  }
}
export function insertionParent(root:LayoutNode, selected:string|null):string|undefined {
  if(selected===null)return isFlow(root)?root.id:undefined;
  const found=locate(root,selected);
  if(!found)return undefined;
  if(isFlow(found.node))return found.node.id;
  if(found.node.element.type==="component"||found.node.element.type==="instance"){
    return found.parent&&isFlow(found.parent)?found.parent.id:undefined;
  }
  return undefined;
}
export function siblingDestination(root:LayoutNode,id:string,delta:-1|1):number|undefined {
  const found=locate(root,id);
  if(!found?.parent||!isFlow(found.parent))return undefined;
  const siblings=found.parent.children??[];
  const index=siblings.findIndex(n=>n.id===id);
  const destination=index+delta;
  return index>=0&&destination>=0&&destination<siblings.length?destination:undefined;
}


// Direct manipulation is limited to visible, non-wrapping semantic flows.
// The returned plan is a proposal only; the Rust transaction validates it.
export type DropZone = "before"|"after"|"inside";
export type DropPlan = {op:"moveNode";node:string;parent:string;index:number};
const visible=(n:LayoutNode)=>!n.layout?.visibility||n.layout.visibility==="visible";
const flowTarget=(n:LayoutNode)=>isFlow(n)&&!n.element.wrap&&visible(n)&&!n.responsive?.length;
function editablePath(root:LayoutNode,id:string):boolean {
  const found=locate(root,id);
  if(!found||!visible(found.node)||found.node.responsive?.length)return false;
  return !found.parent||flowTarget(found.parent)&&editablePath(root,found.parent.id);
}
export function canDrag(root:LayoutNode,id:string):boolean {
  const source=locate(root,id);
  return !!source?.parent&&editablePath(root,id)&&(flowTarget(source.node)||source.node.element.type==="component");
}
export function planDrop(root:LayoutNode,id:string,target:string,zone:DropZone):DropPlan|undefined {
  if(!canDrag(root,id)||id===target||!editablePath(root,target))return;
  const source=locate(root,id)!,hit=locate(root,target)!;
  const parent=zone==="inside"?hit.node:hit.parent;
  if(!parent||!flowTarget(parent)||locate(source.node,parent.id))return;
  const children=parent.children??[];
  const remaining=children.filter(n=>n.id!==id);
  const at=zone==="inside"?remaining.length:remaining.findIndex(n=>n.id===target);
  if(at<0)return;
  const index=at+(zone==="after"?1:0);
  if(parent.id===source.parent!.id&&index===children.findIndex(n=>n.id===id))return;
  return {op:"moveNode",node:id,parent:parent.id,index};
}
// Background/gap drops follow the container's main axis and the actual
// visible child midpoints. The semantic index still includes hidden siblings.
export type FlowChildCenter={id:string;center:number};
export type FlowDropTarget={id:string;zone:DropZone;command:DropPlan|undefined};
export function flowGapDrop(root:LayoutNode,id:string,container:string,position:number,centers:FlowChildCenter[]):FlowDropTarget|undefined {
  const parent=locate(root,container)?.node;
  if(!parent||!flowTarget(parent)||!Number.isFinite(position)||!editablePath(root,container))return;
  const geometry=new Map(centers.filter(c=>Number.isFinite(c.center)).map(c=>[c.id,c.center]));
  const children=(parent.children??[]).filter(n=>n.id!==id&&visible(n)&&geometry.has(n.id));
  const next=children.find(n=>position<geometry.get(n.id)!);
  const target=next?{id:next.id,zone:"before" as const}:{id:container,zone:"inside" as const};
  return {...target,command:planDrop(root,id,target.id,target.zone)};
}

// Empty auto-sized flows otherwise collapse to a zero-area, unreachable drop
// destination. The minimum is editor-only and honors explicit model limits.
export function emptyFlowMin(n:LayoutNode):{minWidth?:number;minHeight?:number} {
  if(!flowTarget(n)||n.children?.length)return {};
  const l=n.layout??{};
  const axis=(size:{mode:string}|undefined,min:number|undefined,max:number|undefined)=>!size||size.mode==="auto"?Math.min(max??Infinity,Math.max(min??0,48)):undefined;
  return {minWidth:axis(l.width,l.minWidth,l.maxWidth),minHeight:axis(l.height,l.minHeight,l.maxHeight)};
}
export function canResize(root:LayoutNode,id:string):boolean {
  const n=locate(root,id)?.node,l=n?.layout;
  return !!n&&canDrag(root,id)&&!l?.weight&&!l?.aspectRatio&&l?.width?.mode!=="fill"&&l?.height?.mode!=="fill";
}
export type Dimensions={width:number;height:number};
// Keep the visible corner grip out of the center of compact auto-sized
// components. Match the CSS grip's 12 dp maximum and 35% axis cap.
export function resizeGripHit(rect:{width:number;height:number;right:number;bottom:number},x:number,y:number,zoom:number):boolean {
  if(![rect.width,rect.height,rect.right,rect.bottom,x,y,zoom].every(Number.isFinite)||zoom<=0||rect.width<=0||rect.height<=0)return false;
  const w=Math.min(12*zoom,rect.width*.35),h=Math.min(12*zoom,rect.height*.35);
  return x>=rect.right-w&&x<=rect.right&&y>=rect.bottom-h&&y<=rect.bottom;
}

export function resizeDimensions(start:Dimensions,dx:number,dy:number,zoom:number,snap:boolean,limits:LayoutNode["layout"]):Dimensions|undefined {
  if(![start.width,start.height,dx,dy,zoom].every(Number.isFinite)||zoom<=0||start.width<0||start.height<0)return;
  const axis=(n:number,min:number|undefined,max:number|undefined)=>Math.min(max??Number.MAX_SAFE_INTEGER,Math.max(min??0,Math.round(n/(snap?10:1))*(snap?10:1)));
  const width=axis(start.width+dx/zoom,limits?.minWidth,limits?.maxWidth);
  const height=axis(start.height+dy/zoom,limits?.minHeight,limits?.maxHeight);
  if(![width,height].every(Number.isFinite)||width<0||height<0)return;
  return {width,height};
}
export type GestureContext={screen:string;revision:number;generation:number|null;zoom:number};
export function sameGestureContext(a:GestureContext,b:GestureContext):boolean {
  return a.screen===b.screen&&a.revision===b.revision&&a.generation===b.generation&&a.zoom===b.zoom;
}
export function canAlign(root:LayoutNode,id:string):boolean {return canDrag(root,id);}
