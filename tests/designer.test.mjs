import {test} from "node:test";
import assert from "node:assert/strict";
import {insertionParent,siblingDestination} from "../src/designer.ts";
const leaf=id=>({id,element:{type:"component"}});
const row={id:"row",element:{type:"row"},children:[leaf("a"),leaf("b")]};
const grid={id:"grid",element:{type:"grid"},children:[leaf("g")]};
const root={id:"root",element:{type:"column"},children:[row,grid,leaf("c")]};
test("nested flow insertion targets selection or its flow parent",()=>{
  assert.equal(insertionParent(root,null),"root");
  assert.equal(insertionParent(root,"row"),"row");
  assert.equal(insertionParent(root,"a"),"row");
  assert.equal(insertionParent(root,"c"),"root");
  for(const id of ["grid","g","missing"])assert.equal(insertionParent(root,id),undefined);
  assert.equal(insertionParent(grid,null),undefined);
});
test("semantic sibling reordering respects parent and boundaries",()=>{
  assert.equal(siblingDestination(root,"a",1),1);
  assert.equal(siblingDestination(root,"b",-1),0);
  assert.equal(siblingDestination(root,"row",1),1);
  for(const [id,delta] of [["a",-1],["b",1],["root",1],["g",1],["missing",-1]])assert.equal(siblingDestination(root,id,delta),undefined);
});


import {canDrag,planDrop,canResize,resizeDimensions,sameGestureContext,canAlign} from "../src/designer.ts";
const gestureRoot={id:"r",element:{type:"row"},children:[
  {id:"a",element:{type:"component"}},
  {id:"b",element:{type:"component"},layout:{width:{mode:"fill"}}},
  {id:"c",element:{type:"column"},children:[{id:"d",element:{type:"component"}}]},
  {id:"wrapped",element:{type:"row",wrap:true},children:[{id:"w",element:{type:"component"}}]},
  {id:"hidden",element:{type:"component"},layout:{visibility:"hidden"}},
  {id:"weighted",element:{type:"component"},layout:{weight:1}},
  {id:"responsive",element:{type:"component"},responsive:[{}]},
  {id:"aspect",element:{type:"component"},layout:{aspectRatio:2}}
]};
test("semantic drag plans use post-removal indexes and reject cycles, roots and unsupported flows",()=>{
  assert.deepEqual(planDrop(gestureRoot,"a","b","after"),{op:"moveNode",node:"a",parent:"r",index:1});
  assert.deepEqual(planDrop(gestureRoot,"b","a","before"),{op:"moveNode",node:"b",parent:"r",index:0});
  assert.deepEqual(planDrop(gestureRoot,"a","c","inside"),{op:"moveNode",node:"a",parent:"c",index:1});
  assert.deepEqual(planDrop(gestureRoot,"d","a","before"),{op:"moveNode",node:"d",parent:"r",index:0});
  for(const [a,b,z] of [["a","b","before"],["c","d","before"],["r","a","after"],["a","wrapped","inside"],["w","a","before"],["hidden","a","before"],["a","hidden","after"],["a","b","inside"]])assert.equal(planDrop(gestureRoot,a,b,z),undefined);
  assert.equal(canDrag(gestureRoot,"a"),true);assert.equal(canAlign(gestureRoot,"r"),false);
});
test("resize uses logical dp, optional snap, constraints and explicit unsupported policies",()=>{
  assert.equal(canResize(gestureRoot,"a"),true);for(const id of ["b","weighted","responsive","aspect","hidden","r","w"])assert.equal(canResize(gestureRoot,id),false);
  assert.deepEqual(resizeDimensions({width:100,height:50},12,7,.5,true,{}),{width:120,height:60});
  assert.deepEqual(resizeDimensions({width:100,height:50},100,-100,1,false,{maxWidth:110,minHeight:20}),{width:110,height:20});
  assert.equal(resizeDimensions({width:100,height:50},1,1,0,false,{}),undefined);
});
test("gesture identity includes screen, revision, native session and zoom",()=>{
  const a={screen:"r",revision:4,generation:1,zoom:.5};
  assert.equal(sameGestureContext(a,{...a}),true);
  for(const b of [{revision:5},{generation:2},{screen:"s"},{zoom:1}])assert.equal(sameGestureContext(a,{...a,...b}),false);
});
