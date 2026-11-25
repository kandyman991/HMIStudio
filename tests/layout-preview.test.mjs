import {test} from "node:test";
import assert from "node:assert/strict";
import {nodeStyle,sizeCss,previewLimit,parseFixed,fitPreview,changeZoom} from "../src/layout-preview.ts";
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
  assert.equal(s.justifyContent,"space-between");assert.equal(s.alignItems,"flex-end");assert.equal(s.flexWrap,"wrap");assert.equal(s.alignContent,"flex-start");
  assert.equal(s.width,240);assert.equal(s.minWidth,100);assert.equal(s.maxHeight,80);assert.equal(s.paddingLeft,8);
  assert.equal(s.flexGrow,2);assert.equal(s.flexBasis,0);assert.equal(s.alignSelf,"center");assert.equal(s.overflow,"hidden");assert.equal(s.aspectRatio,2);
  assert.equal(nodeStyle({element:{type:"column",spacing:7}}).gap,7);
});
test("visibility and unsupported features are explicit",()=>{
  assert.equal(nodeStyle({element:{type:"row"},layout:{visibility:"collapsed"}}).display,"none");
  const hidden=nodeStyle({element:{type:"component"},layout:{visibility:"hidden"}});assert.equal(hidden.visibility,"hidden");assert.equal(hidden.pointerEvents,"none");
  assert.equal(previewLimit({element:{type:"grid"},responsive:[{}],bindings:[{}]}).length,3);
  assert.equal(previewLimit({element:{type:"component",kind:"text"}}).length,0);
  assert.equal(previewLimit({element:{type:"component",kind:"slider"}}).length,1);
});
test("numeric edits reject blank, negative and nonfinite values",()=>{
  for(const v of [""," ","-1","NaN","Infinity","abc","0x10","0b10"])assert.equal(parseFixed(v),undefined);
  assert.equal(parseFixed("0"),0);assert.equal(parseFixed("12.5"),12.5);
});

test("fit and zoom are bounded presentation scaling, not logical size changes",()=>{
  assert.equal(fitPreview(1000,700),1);
  assert.equal(fitPreview(700,500),.8);
  assert.equal(fitPreview(100,80),.25);
  assert.equal(fitPreview(0,0),1);
  assert.equal(changeZoom(.25,-.1),.25);
  assert.equal(changeZoom(2,.1),2);
  assert.equal(changeZoom(.8,.1),.9);
});
test("unresolved style, events and localized text cannot masquerade as runtime preview",()=>{
  assert.deepEqual(previewLimit({element:{type:"component",kind:"text"},style:"s",events:[{}],properties:{text:{key:"greeting"}}}),["Named style not applied","Events not executed","Localized or nonliteral text not resolved"]);
});

import {contractFor,defaultContract,defaultLayout,geometry,localizedLabel,parseCoordinate,parsePoints,enumPairs} from "../src/v2.ts";
test("v2 palette admits only compatible writable string and discrete enum targets",()=>{
 const project={tags:[{id:"str",name:"Text",dataType:"string",access:"readWrite"},{id:"ro",name:"Read",dataType:"string"},{id:"word",name:"Word",dataType:"uint64",access:"readWrite"},{id:"wo",name:"Write",dataType:"uint64",access:"write"}],v2:{bindings:{enumerations:[{id:"enum",name:"Exact",dataType:"uint64",entries:[{label:"Max",value:18446744073709551615n}]}]}}};
 assert.deepEqual(defaultContract("textInput",project),{kind:"textInput",tag:"str",maxLength:80});
 assert.deepEqual(defaultContract("dropDown",project),{kind:"dropDown",tag:"word",enumeration:"enum"});
 assert.equal(enumPairs(project).length,1);
 assert.equal(defaultContract("textInput",{}),undefined);
 assert.equal(defaultContract("dropDown",{}),undefined);
 assert.deepEqual(defaultLayout("line").width,{mode:"fixed",value:120});
 assert.equal(project.v2.bindings.enumerations[0].entries[0].value,18446744073709551615n);
});
test("v2 geometry and numeric edits are finite and reject malformed coordinates",()=>{
 for(const v of [""," ","NaN","Infinity","0x10","1e999","1,2"])assert.equal(parseCoordinate(v),undefined);
 assert.equal(parseCoordinate("-2.5"),-2.5);
 assert.deepEqual(parsePoints("0,0\n4,0\n-2,6"),[{x:0,y:0},{x:4,y:0},{x:-2,y:6}]);
 assert.equal(parsePoints("0,0\n1,2"),undefined);
 assert.equal(parsePoints("0,0\n1,2\n,3"),undefined);
 for(const k of ["line","arrow","polygon"]){
   const c=defaultContract(k,{}),g=geometry(c);assert.ok(g);assert.ok(g.viewBox.split(" ").map(Number).every(Number.isFinite));
   assert.equal(previewLimit({element:{type:"component",kind:k}}).length,0);
   if(k==="arrow")assert.deepEqual(g.head[0],c.end);
 }
 assert.equal(geometry({kind:"arrow",start:{x:1,y:1},end:{x:1,y:1},strokeWidth:1,strokeColor:"#000000",headLength:1}),undefined);
});
test("v2 preview resolves labels by default locale and isolates contract lookup",()=>{
 const contract=defaultContract("line",{}),p={localization:{defaultLocale:"it",strings:{hello:{it:"Ciao",en:"Hello"}}},v2:{components:{components:[{id:"n-line",contract}]}}};
 assert.equal(contractFor(p,"n-line"),contract);
 assert.equal(contractFor(p,"missing"),undefined);
 assert.equal(localizedLabel({textKey:"hello"},p),"Ciao");
 assert.equal(localizedLabel({textKey:"missing"},p),"[missing]");
 assert.equal(localizedLabel("literal",p),"literal");
 const style=nodeStyle({element:{type:"component",kind:"line"},layout:{position:{x:12,y:8}}},{type:"absolute"});
 assert.equal(style.position,"absolute");assert.equal(style.left,12);assert.equal(style.top,8);
});
