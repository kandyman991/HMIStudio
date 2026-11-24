import { test } from "node:test";
import assert from "node:assert/strict";
import { DEFAULTS, restorePreferences, preset, clampSize, reconcileTabs, openTab, closeTab, cycleTab } from "../src/workspace.ts";

test("missing, corrupt and unsupported preferences reset safely",()=>{
  for(const value of [null,"{","null",'{"version":99,"left":false}']) assert.deepEqual(restorePreferences(value,null),DEFAULTS);
});
test("legacy visibility migrates; v2 sizes and types are sanitized",()=>{
  assert.deepEqual(restorePreferences(null,'{"left":false,"right":true,"diagnostics":false}'),{...DEFAULTS,left:false,diagnostics:false});
  const p=restorePreferences('{"version":2,"left":false,"right":0,"swapped":true,"explorerSize":999,"inspectorSize":-9,"diagnosticsSize":"5","arbitrary":"discard"}',null);
  assert.deepEqual(p,{...DEFAULTS,left:false,swapped:true,explorerSize:6,inspectorSize:1});
  assert.equal(clampSize(NaN),4);
});
test("presets are independent presentation values and round-trip",()=>{
  for(const name of ["standard","design","diagnostics"]){const p=preset(name);assert.deepEqual(restorePreferences(JSON.stringify(p),null),p);}
  assert.equal(preset("design").left,false);
  assert.equal(preset("diagnostics").diagnosticsSize,6);
});
test("tabs open once, select, cycle and close without touching project",()=>{
  let t={open:[],active:null};
  t=openTab(t,"a");t=openTab(t,"b");t=openTab(t,"a");
  assert.deepEqual(t,{open:["a","b"],active:"a"});
  t=cycleTab(t,-1);assert.equal(t.active,"b");
  t=closeTab(t,"b");assert.deepEqual(t,{open:["a"],active:"a"});
  t=closeTab(t,"a");assert.deepEqual(t,{open:[],active:null});
});
test("history reconciliation removes missing screens and duplicate tab IDs",()=>{
  assert.deepEqual(reconcileTabs({open:["a","b","b","c"],active:"a"},["b","c"]),{open:["b","c"],active:"b"});
  assert.deepEqual(reconcileTabs({open:[],active:null},["a"]),{open:[],active:null});
});
