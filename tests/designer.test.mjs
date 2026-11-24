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
