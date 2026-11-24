import {test} from "node:test";
import assert from "node:assert/strict";
import {nodeStyle,sizeCss,previewLimit,parseFixed} from "../src/layout-preview.ts";
test("fixed, fill and intrinsic sizes map without coercion",()=>{
  assert.equal(sizeCss({mode:"fixed",value:0}),0);
  assert.equal(sizeCss({mode:"fill",fraction:0.5}),"50%");
  assert.equal(sizeCss({mode:"fill"}),"100%");
  assert.equal(sizeCss({mode:"auto"}),"auto");
  assert.equal(sizeCss({mode:"fixed",value:-1}),"auto");
  assert.equal(sizeCss({mode:"fill",fraction:2}),"auto");
});
test("flow arrangement, padding, constraints, wrap and weight are mapped",()=>{
  const s=nodeStyle({element:{type:"row",spacing:12,alignment:"end",arrangement:"spaceBetween",wrap:true},layout:{width:{mode:"fixed",value:240},minWidth:100,maxHeight:80,padding:{start:8},weight:2,crossAlign:"center",clip:true,aspectRatio:2}},{type:"column"});
  assert.equal(s.display,"flex");assert.equal(s.flexDirection,"row");assert.equal(s.gap,0);
  assert.equal(s.justifyContent,"space-between");assert.equal(s.alignItems,"flex-end");assert.equal(s.flexWrap,"wrap");
  assert.equal(s.width,240);assert.equal(s.minWidth,100);assert.equal(s.maxHeight,80);assert.equal(s.paddingLeft,8);
  assert.equal(s.flexGrow,2);assert.equal(s.flexBasis,0);assert.equal(s.alignSelf,"center");assert.equal(s.overflow,"hidden");assert.equal(s.aspectRatio,2);
  assert.equal(nodeStyle({element:{type:"column",spacing:7}}).gap,7);
});
test("visibility and unsupported features are explicit",()=>{
  assert.equal(nodeStyle({element:{type:"row"},layout:{visibility:"collapsed"}}).display,"none");
  assert.equal(nodeStyle({element:{type:"component"},layout:{visibility:"hidden"}}).visibility,"hidden");
  assert.equal(previewLimit({element:{type:"grid"},responsive:[{}],bindings:[{}]}).length,3);
  assert.equal(previewLimit({element:{type:"component",kind:"text"}}).length,0);
  assert.equal(previewLimit({element:{type:"component",kind:"slider"}}).length,1);
});
test("numeric edits reject blank, negative and nonfinite values",()=>{
  for(const v of [""," ","-1","NaN","Infinity","abc"])assert.equal(parseFixed(v),undefined);
  assert.equal(parseFixed("0"),0);assert.equal(parseFixed("12.5"),12.5);
});
