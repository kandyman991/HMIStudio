// Pure selection rules for the first semantic designer slice.
// Grid, absolute, scroll and box placement have their own semantics and are
// intentionally not inferred here. The Rust engine remains authoritative.
export type LayoutNode = {
  id: string;
  element: { type: string };
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
