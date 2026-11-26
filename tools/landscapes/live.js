import {addFantasy} from './fantasy.js';
import {addClouds} from './clouds.js';
import {makeBreeze} from './atmosphere.js';
import * as THREE from 'three';
import { HDRLoader } from 'three/addons/loaders/HDRLoader.js';
import sunrise from './assets/qwantani_sunrise_puresky_1k.hdr';
import dusk from './assets/qwantani_dusk_2_puresky_1k.hdr';
import grainUrl from './assets/siding-grain.png';
import { geometry as sceneFactories } from 'carbon:geometry';
import definitions from './sources.json';
import { cameraFor } from './source-config.mjs';

const params = new URLSearchParams(location.search);
const id = params.get('source');
const camera = cameraFor(definitions, { source: id, camera: params.get('camera') });
const spec = { ...definitions[id], build: sceneFactories[id] };
const canvas = document.querySelector('canvas');
const alpine=id==='alpine', farm=id==='farmland', marsh=id==='marsh';
for(const link of document.querySelectorAll('nav a')) link.setAttribute('aria-current', (link.hash === '#'+id)?'page':'false');
addEventListener('hashchange',()=>location.reload());
canvas.setAttribute('aria-label',spec.name+' animated landscape');
const control = document.querySelector('button');
const status = document.querySelector('[role=status]');
const preference = matchMedia('(prefers-reduced-motion: reduce)');
let paused = preference.matches;
const renderer = new THREE.WebGLRenderer({canvas, antialias:true, alpha:false, powerPreference:'low-power'});
renderer.shadowMap.enabled = true;
renderer.shadowMap.type = THREE.PCFShadowMap;
renderer.shadowMap.autoUpdate = false;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = farm?1.03:1.0;
const scene = new THREE.Scene();
scene.fog = new THREE.FogExp2(0xc5aaa0, 0.014);
if(alpine)scene.fog=new THREE.FogExp2(0x9aacc1,.009);
const cam = new THREE.PerspectiveCamera(42, 1, 0.15, 180);
const aim = new THREE.Vector3(...camera.target);
cam.position.set(...camera.eye);
const cameraOffset = cam.position.clone().sub(aim);
const vertical = new THREE.Vector3(0, 1, 0);
cam.lookAt(aim);
const sky = await new HDRLoader().loadAsync(spec.dusk?dusk:sunrise);
sky.mapping = THREE.EquirectangularReflectionMapping;
scene.environment = sky;
scene.environmentIntensity = farm?.30:.23;
scene.background = sky;
scene.backgroundRotation.y = 2.3;
scene.environmentRotation.y = 2.3;
scene.backgroundIntensity = .32;
const sun = new THREE.DirectionalLight(0xffce86, 3.15);
sun.position.set(...spec.sun);
if(alpine){sun.position.set(-30,19,-6);sun.color.set(0xffc8aa);sun.intensity=2.6;scene.environmentIntensity=.52;scene.backgroundIntensity=.62;}
if(marsh){scene.fog=new THREE.FogExp2(0x837993,.017);scene.backgroundIntensity=.105;scene.environmentIntensity=.18;sun.color.set(0xffbeac);sun.intensity=.89;renderer.toneMappingExposure=.90;}
sun.target.position.set(0,0,-3);
sun.castShadow = true;
sun.shadow.mapSize.set(2048,2048);
Object.assign(sun.shadow.camera,{left:-32,right:32,top:29,bottom:-29,near:1,far:90});
sun.shadow.camera.updateProjectionMatrix();
sun.shadow.normalBias=.035;
sun.shadow.bias=-.00016;
scene.add(sun,sun.target,new THREE.HemisphereLight(marsh?0xc9c8ed:0xe1efff,0x987b53,marsh?.42:.70));

const blocks = [];
const emitBox=(x,y,z,w,h,d,color,ry=0,motion=null)=>blocks.push({x,y,z,w,h,d,color,ry,motion});
const world = spec.build(emitBox) ?? {};
addFantasy(emitBox,id);
const cycleSeconds=8;
const turn=Math.PI*2;
const breeze=makeBreeze(scene, cam, params.get('camera')==='overhead', spec.dusk, spec.sun, id);
const cloudBoxes=[];
addClouds((x,y,z,w,h,d,color)=>cloudBoxes.push({x,y,z,w,h,d,color,ry:0}),id);
// Bake a coarse sky-visibility estimate once. Only indirect light is attenuated;
// sunshine and moving silhouettes continue to use the renderer's light/shadow path.
const cell=.65, occupied=new Set();
function key(x,y,z){return (x+512)*1048576+(y+160)*1024+z+512;}
for(const b of blocks){
 if(b.motion || Math.min(b.h,b.w,b.d)<.42 || b.w*b.h*b.d>1700)continue;
 const c=Math.cos(b.ry),s=Math.sin(b.ry),rx=(Math.abs(c*b.w)+Math.abs(s*b.d))/2,rz=(Math.abs(s*b.w)+Math.abs(c*b.d))/2;
 for(let i=Math.ceil((b.x-rx)/cell);i<=(b.x+rx)/cell;i++)
  for(let k=Math.ceil((b.z-rz)/cell);k<=(b.z+rz)/cell;k++){
   const dx=i*cell-b.x,dz=k*cell-b.z;
   if(Math.abs(c*dx-s*dz)>b.w/2||Math.abs(s*dx+c*dz)>b.d/2)continue;
   for(let j=Math.ceil((b.y-b.h/2)/cell);j<=(b.y+b.h/2)/cell;j++)occupied.add(key(i,j,k));
  }
}
function skyAccess(b){
 if(b.motion)return 1;
 const y=b.y+b.h*.5+.52;
 let occlusion=0;
 for(const [dx,dz] of [[.83,.28],[-.7,.52],[.35,-.81],[-.61,-.67]]){
  for(const r of [1,2,4]){
   if(occupied.has(key(Math.round((b.x+dx*r)/cell),Math.round((y+r*.65)/cell),Math.round((b.z+dz*r)/cell)))){
    occlusion+=.12/r**.35;break;
   }
  }
 }
 return 1-occlusion;
}
const geometry = new THREE.BoxGeometry();
const woodGrain=await new THREE.TextureLoader().loadAsync(grainUrl);
woodGrain.wrapS=woodGrain.wrapT=THREE.RepeatWrapping;
woodGrain.magFilter=THREE.NearestFilter;
const timberColors=new Set(('#986b43 #a6774a #a27245 #8f623e #aa794b #997547 #a07b4b #a68050 #907049 #98754c #533c30 #664b35 #bc925e #87644b #937456 #a07b59 #81614a #95775d #8e4935 #974d39 #a4563e #9e5038 #ab5940 #884633').split(' '));
const stone = new THREE.MeshStandardMaterial({roughness:0.96,metalness:0});
stone.onBeforeCompile = (shader)=>{
 shader.uniforms.woodGrain={value:woodGrain};
 shader.uniforms.fieldTime=time;
 shader.vertexShader = 'attribute float isTimber;attribute float skyAccess;varying float skyLight; varying float timber; varying vec3 worldP; varying vec3 localP;varying vec3 surfaceGrid;\n'+shader.vertexShader.replace('#include <project_vertex>','#include <project_vertex>\nworldP=(modelMatrix * instanceMatrix * vec4(transformed,1.)).xyz; localP=position;surfaceGrid=position*vec3(length(instanceMatrix[0].xyz),length(instanceMatrix[1].xyz),length(instanceMatrix[2].xyz));timber=isTimber;skyLight=skyAccess;');
 if(farm){
  shader.vertexShader='uniform float fieldTime;\n'+shader.vertexShader.replace('skyLight=skyAccess;',`skyLight=skyAccess;
   if(worldP.x < -7.9 && worldP.x > -18.4 && worldP.z > -11.6 && worldP.z < 3.2 && worldP.y>0.09 && worldP.y<1.12){
    float wave=sin(worldP.x*.76+worldP.z*.47-fieldTime*2.)+.42*sin(worldP.z*.91-fieldTime*3.);
    vec3 wind=vec3(.08,.0,.045)*wave*pow(max(0.,worldP.y-.08),1.5);
    gl_Position+=projectionMatrix*viewMatrix*vec4(wind,0.);
   }`);
 }
 shader.fragmentShader = 'uniform float fieldTime;uniform sampler2D woodGrain; varying float skyLight;varying float timber; varying vec3 worldP; varying vec3 localP;varying vec3 surfaceGrid;\n'+shader.fragmentShader.replace('#include <aomap_fragment>','#include <aomap_fragment>\nreflectedLight.indirectDiffuse*=skyLight;').replace('#include <color_fragment>',`#include <color_fragment>
 vec3 voxel=floor(surfaceGrid*7.999+0.017);
 if(timber>.5){
  vec3 n=abs(normalize(cross(dFdx(worldP),dFdy(worldP))));
  vec2 st=n.y>.6?worldP.xz:(n.x>.6?worldP.zy:worldP.xy);
  diffuseColor.rgb*=.70+.60*texture2D(woodGrain,st*vec2(.42,.32)).r;
 }
 float grain=fract(sin(dot(mod(voxel,129.),vec3(17.137,39.43,71.35)))*1453.41);
 float farFade=1.0-smoothstep(.10,.38,length(fwidth(worldP)));
 // Wide cloud shadows brush terrain gradually, with the same phase on wrap.
 float shadowWave=sin(worldP.x*.18+worldP.z*.09-fieldTime)+.43*sin(worldP.z*.19-fieldTime);
 diffuseColor.rgb *= 1.-.045*smoothstep(.37,1.20,shadowWave);
 diffuseColor.rgb *= 1.0+(grain-.5)*.31*farFade;
 vec3 tile=floor((voxel+vec3(17.,3.,7.))/3.);
 float tileTone=fract(sin(dot(mod(tile,127.),vec3(31.13,15.37,51.71)))*721.79);
 diffuseColor.rgb*=1.+(tileTone-.5)*.145*farFade;
 
 if(diffuseColor.g>diffuseColor.r*.94 && diffuseColor.g>diffuseColor.b*1.45){
   diffuseColor.rgb*=mix(vec3(.91,.98,.90),vec3(1.08,1.04,.96),step(.49,tileTone));
   float dew=smoothstep(.976,.998,grain)*max(0.,localP.y+.3)*farFade;
   diffuseColor.rgb+=vec3(.19,.22,.18)*dew;
 }
 `);
};
const amber = new THREE.MeshStandardMaterial({color:0xffbb73,emissive:0xff942c,emissiveIntensity:1.2});
const transform = new THREE.Object3D();
const tint = new THREE.Color();
function drawBoxes(items, material, useColors){
 if(!items.length) return;
 const geom=geometry.clone();
 if(useColors)geom.setAttribute('isTimber',new THREE.InstancedBufferAttribute(new Float32Array(items.map(b=>timberColors.has(b.color)?1:0)),1));
 if(useColors)geom.setAttribute('skyAccess',new THREE.InstancedBufferAttribute(new Float32Array(items.map(skyAccess)),1));
 const mesh = new THREE.InstancedMesh(geom, material,items.length);
 items.forEach((b,i)=>{
  transform.position.set(b.x,b.y,b.z); transform.scale.set(b.w,b.h,b.d); transform.rotation.set(0,b.ry,0); transform.updateMatrix();
  mesh.setMatrixAt(i,transform.matrix);
  if(useColors) mesh.setColorAt(i,tint.set(b.color));
 });
 mesh.castShadow=true; mesh.receiveShadow=true; scene.add(mesh);return mesh;
}
if(cloudBoxes.length){
 const material=new THREE.MeshLambertMaterial({color:cloudBoxes[0].color,emissive:marsh?0x584660:0xc5b4ae});
 const horizon=drawBoxes(cloudBoxes,material,false);
 horizon.castShadow=false;horizon.receiveShadow=false;
}
const lit=b=>String(b.color).toLowerCase()==='#ffb455';
drawBoxes(blocks.filter(b=>!lit(b)&&!b.motion),stone,true);
drawBoxes(blocks.filter(lit),amber,false);
const moving=blocks.filter(b=>b.motion&&!lit(b));
const movingMesh=drawBoxes(moving,stone,true);
const pos=new THREE.Vector3(),pivot=new THREE.Vector3(),orientation=new THREE.Quaternion();
const axisX=new THREE.Vector3(1,0,0),axisY=new THREE.Vector3(0,1,0),axisZ=new THREE.Vector3(0,0,1);
const axleDirection=new THREE.Vector3();
function animateModels(phase){
 const t=phase*turn;
 if(!movingMesh)return;
 moving.forEach((b,i)=>{
  const m=b.motion;
  const travel=1.05*(1-Math.cos(t));
  const axis=m.drive?axleDirection.set(Math.cos(m.heading),0,-Math.sin(m.heading)):m.axis==='x'?axisX:m.axis==='y'?axisY:axisZ;
  const angle=m.drive?(m.radius?travel/m.radius:0):m.spin?t*Math.sign(m.spin):m.amplitude*(.5+.5*Math.sin(t+m.phase));
  pos.set(b.x,b.y,b.z);
  if(m.pivot){pivot.fromArray(m.pivot);pos.sub(pivot).applyAxisAngle(axis,angle).add(pivot);}
  if(m.drive){pos.x+=Math.sin(m.heading)*travel;pos.z+=Math.cos(m.heading)*travel;}
  transform.position.copy(pos);transform.scale.set(b.w,b.h,b.d);
  transform.quaternion.setFromAxisAngle(axis,angle).multiply(orientation.setFromAxisAngle(axisY,b.ry));
  transform.updateMatrix();movingMesh.setMatrixAt(i,transform.matrix);
 });movingMesh.instanceMatrix.needsUpdate=true;
}

const reflectCam = new THREE.PerspectiveCamera();
const reflection = new THREE.WebGLRenderTarget(768,432,{type:THREE.HalfFloatType,depthBuffer:true});
const time = {value:0};
const waterMat = new THREE.ShaderMaterial({
 lights:true, uniforms:{...THREE.UniformsUtils.clone(THREE.UniformsLib.lights),mirrorMap:{value:reflection.texture}, mirrorProjection:{value:new THREE.Matrix4()},waveStrength:{value:marsh?.30:alpine?.53:1},time},
 vertexShader:`varying vec3 pos; varying vec4 projection; uniform mat4 mirrorProjection;
 #include <common>
 #include <shadowmap_pars_vertex>
 void main(){vec4 worldPosition=modelMatrix*vec4(position,1.);pos=worldPosition.xyz;projection=mirrorProjection*worldPosition;vec4 mvPosition=viewMatrix*worldPosition;gl_Position=projectionMatrix*mvPosition;
 vec3 transformedNormal=normalize(normalMatrix*normal);
 #include <shadowmap_vertex>
 }`,
 fragmentShader:`varying vec3 pos; varying vec4 projection; uniform sampler2D mirrorMap; uniform float time; uniform float waveStrength;
 #include <common>
 #include <packing>
 #include <lights_pars_begin>
 #include <shadowmap_pars_fragment>
 #include <shadowmask_pars_fragment>
 void main(){
 vec2 p=pos.xz; float s=sin(p.y*4.1+p.x*1.1+time*2.+.7*sin(p.x*.78+time));
 float c=sin(p.x*3.8+p.y*8.1-time*2.+.85*sin(p.y*.9));
 vec2 uv=projection.xy/projection.w*.5+.5;
 float dist=distance(cameraPosition,pos);
 vec2 wave=vec2(s*.0026+c*.0015,s*.0033+c*.0031)*min(1.,15./dist)*waveStrength;
 vec3 reflected=texture2D(mirrorMap,clamp(uv+wave,.001,.999)).rgb;
 float fresnel=pow(1.-max(0.,normalize(cameraPosition-pos).y),2.);
 vec3 base=mix(vec3(.044,.133,.14),vec3(.114,.24,.218),.5+.5*sin(p.x*.68+p.y*.41));
 reflected=min(reflected,vec3(2.0))*vec3(.58,.72,.80);
 float visibility=getShadowMask();
 vec3 col=mix(base*(.38+.34*visibility),reflected,.24+.60*fresnel);
 float ripple=smoothstep(.84,.995,s*.64+c*.36);
 col+=vec3(.17,.14,.085)*visibility*ripple*waveStrength*(1.-smoothstep(15.,65.,dist));
 gl_FragColor=vec4(col,1.);
 #include <tonemapping_fragment>
 #include <colorspace_fragment>
 }`
});
const water = new THREE.Mesh(new THREE.PlaneGeometry(155,155),waterMat);
water.receiveShadow=true;water.rotation.x=-Math.PI/2; scene.add(water);
const mist = new THREE.Group();scene.add(mist);
const smokeGeo=new THREE.BoxGeometry(1,1,1);
const chimney=world?.chimney;
for(let i=0;chimney&&i<6;i++){
 const puff=new THREE.Mesh(smokeGeo,new THREE.MeshBasicMaterial({color:0xc8c2b7,transparent:true,depthWrite:false}));
 mist.add(puff);
}
function smoke(phase){
 mist.children.forEach((p,i)=>{
  const age=(phase+i/6)%1;
  const [x,y,z]=chimney;
  p.position.set(x+age*1.75,y+.15+age*2.25,z+.25*Math.sin(age*3.1));
  p.scale.setScalar(.18+age*.46); p.rotation.y=.2+age*.75;
  p.material.opacity=.18*Math.sin(Math.PI*age);
 });
}
// Thin layers of ground fog drift through the pastures. The shader fades both
// edges before the alpha blend, so no rectangular silhouettes appear.
const haze=new THREE.Group();
const motes = new THREE.Group();
scene.add(motes);
const anchors=world.leafOrigins || world.glowOrigins || [];
const isLeaf=id==='orchard';
const leafShape=new THREE.Shape();
leafShape.moveTo(-.14,0);leafShape.lineTo(-.045,.07);leafShape.lineTo(.065,.055);
leafShape.lineTo(.15,0);leafShape.lineTo(.02,-.065);leafShape.closePath();
const moteGeometry=isLeaf?new THREE.ShapeGeometry(leafShape):new THREE.IcosahedronGeometry(.021,0);
anchors.forEach((a,i)=>{
 for(let j=0;j<(isLeaf?2:4);j++){
  const material=isLeaf?new THREE.MeshStandardMaterial({color:[0xb36524,0xc48a39,0x974824][(i+j)%3],side:THREE.DoubleSide}):
   new THREE.MeshBasicMaterial({color:0xfbdea1,transparent:true,depthWrite:false});
  const mote=new THREE.Mesh(moteGeometry,material);
  mote.userData={a,phase:(i*.618+j*.413)%1,speed:.071+(i%4)*.008+j*.003};
  motes.add(mote);
 }
});
for(const a of world.lanternOrigins || []){
 const lamp=new THREE.PointLight(0xffa64b,6,4.5,2);
 lamp.position.set(...a);scene.add(lamp);
}
function animateSeason(cycle){
 for(const mote of motes.children){
  const {a,phase}=mote.userData;
  if(isLeaf){
   const age=(phase+cycle)%1, angle=phase*17+age*9, bottom=Number.isFinite(a[3])?a[3]+.08:a[1]-1.6;
   mote.position.set(a[0]+age*.66+.19*Math.sin(angle), THREE.MathUtils.lerp(a[1],bottom,age),a[2]+age*.33+.13*Math.cos(angle));
   mote.scale.setScalar(Math.min(1,age*14,(1-age)*12));
   mote.rotation.set(angle*.72,angle,Math.cos(angle)*.6);
  }else{
   const phaseAngle=(phase+cycle)*turn;
   mote.position.set(a[0]+.8*Math.sin(phaseAngle),a[1]+.2*Math.cos(phaseAngle+phase),a[2]+.61*Math.sin(phaseAngle-phase));
   mote.material.opacity=Math.pow(.5+.5*Math.sin(phaseAngle*3+phase*11),3)*.92;
  }
 }
}
if(farm){
 scene.add(haze);
 for(const [x,y,z,w,h] of [[-9,1,-17,43,2.4],[3,1.1,-9,30,1.75],[-5,.85,-2,22,1.25]]){
  const material=new THREE.ShaderMaterial({transparent:true,depthWrite:false,uniforms:{time},
   vertexShader:'varying vec2 coord;void main(){coord=uv;gl_Position=projectionMatrix*modelViewMatrix*vec4(position,1.0);}',
   fragmentShader:`varying vec2 coord;uniform float time;
   void main(){vec2 u=coord;float edge=sin(3.14159*u.x)*pow(sin(3.14159*u.y),2.);
   float wave=.60+.18*sin(17.*u.x+2.*u.y+time)+.14*sin(39.*u.x+7.*u.y-time);
   gl_FragColor=vec4(.96,.85,.69,edge*wave*.27);
   #include <colorspace_fragment>
   }`});
  const veil=new THREE.Mesh(new THREE.PlaneGeometry(w,h),material);
  veil.position.set(x,y,z);veil.rotation.y=.35;haze.add(veil);
 }
}
water.visible=!!spec.water;mist.visible=!!chimney;
const waterline=new THREE.Plane(new THREE.Vector3(0,1,0),.015),clip=new THREE.Vector4(),farCorner=new THREE.Vector4();
function updateMirror(){
 water.visible=false; mist.visible=false; cam.visible=false;
 reflectCam.copy(cam,false); reflectCam.position.y*=-1;
 const look=cam.getWorldDirection(new THREE.Vector3());look.y*=-1;
 reflectCam.up.set(0,-1,0);reflectCam.lookAt(reflectCam.position.clone().add(look));reflectCam.updateMatrixWorld();
 waterMat.uniforms.mirrorProjection.value.multiplyMatrices(reflectCam.projectionMatrix,reflectCam.matrixWorldInverse);
 // Exclude submerged geometry from the virtual camera, including pier footings.
 const plane=waterline.clone().applyMatrix4(reflectCam.matrixWorldInverse);
 clip.set(plane.normal.x,plane.normal.y,plane.normal.z,plane.constant);
 const e=reflectCam.projectionMatrix.elements;
 farCorner.set((Math.sign(clip.x)+e[8])/e[0],(Math.sign(clip.y)+e[9])/e[5],-1,(1+e[10])/e[14]);
 clip.multiplyScalar(2/clip.dot(farCorner));
 e[2]=clip.x;e[6]=clip.y;e[10]=clip.z+1;e[14]=clip.w;
 renderer.setRenderTarget(reflection); renderer.render(scene,reflectCam);renderer.setRenderTarget(null);
 water.visible=true;mist.visible=!!chimney;cam.visible=true;
}
let tick=0,previous=0,elapsed=0,nextPaint=0;
const intervals=[];
let lastMirror=-Infinity;
function draw(){
 const phase=(elapsed/cycleSeconds)%1;
 if(chimney)smoke(phase);
 animateModels(phase);animateSeason(phase);time.value=phase*turn;
 breeze(phase);
 // Shadows move with the actor on each displayed frame. Static views reuse their map.
 if(moving.length)renderer.shadowMap.needsUpdate=true;
 if(spec.water && (lastMirror===-Infinity || (moving.length && elapsed-lastMirror>=1/12))){
  updateMirror();lastMirror=elapsed;
 }
 renderer.render(scene,cam);
}
function resize(){
 const w=innerWidth,h=innerHeight;
 const pixelRatio=Math.min(devicePixelRatio,1.5, Math.sqrt(1600000/(w*h)));
 renderer.setPixelRatio(pixelRatio);renderer.setSize(w,h,false);cam.aspect=w/h;
 // Protect the authored horizontal composition in shallow launcher headers.
 // A continuous lens change avoids a jump as the window crosses a breakpoint.
 const baseLens=THREE.MathUtils.degToRad(spec.fov||42);
 const framing=THREE.MathUtils.clamp(1.84/cam.aspect,.48,1.55);
 cam.fov=THREE.MathUtils.radToDeg(2*Math.atan(Math.tan(baseLens/2)*framing));
 cam.updateProjectionMatrix();cam.updateMatrixWorld();
 reflection.setSize(Math.min(900,Math.max(256,Math.round(w*.55))),Math.min(600,Math.max(160,Math.round(h*.55))));
 renderer.shadowMap.needsUpdate=true;lastMirror=-Infinity;draw();
}
function frame(now){
 if(paused || document.hidden){tick=0;return;}
 if(!previous || now>=nextPaint-.5){
  if(previous) { const delta=now-previous; intervals.push(delta);if(intervals.length>180) intervals.shift();elapsed+=Math.min(delta,150)/1000; }
  nextPaint=Math.max(nextPaint+1000/30,now+3);
  if(!previous) nextPaint=now+1000/30;
  previous=now;draw();
 }
 tick=requestAnimationFrame(frame);
}
function sync(){
 cancelAnimationFrame(tick);tick=0;previous=0;nextPaint=0;
 control.textContent=paused?'Play motion':'Pause motion';control.setAttribute('aria-pressed',String(!paused));
 if(!paused && !document.hidden)tick=requestAnimationFrame(frame);
}
control.onclick=()=>{paused=!paused;sync();};
preference.addEventListener('change',()=>{paused=preference.matches;sync();});
document.addEventListener('visibilitychange',sync);
window.addEventListener('resize',resize);
resize();sync();status.hidden=true;
window.sceneStats=()=>({blocks:blocks.length,draws:renderer.info.render.calls,triangles:renderer.info.render.triangles,intervals:[...intervals],elapsed,paused});


// An eight-second periodic camera path advances with the atmosphere. Baking
// [0, 1) keeps the last-to-first transition a single ordinary frame interval.
window.paintScenicFrame = (fraction) => {
  paused = true;
  sync();
  const phase = fraction % 1;
  elapsed = phase * cycleSeconds;
  const eased = (1 - Math.cos(turn * phase)) * 0.5;
  cam.position.copy(cameraOffset).applyAxisAngle(vertical, eased * 0.018).add(aim);
  cam.lookAt(aim);
  cam.updateMatrixWorld();
  lastMirror = -Infinity;
  renderer.shadowMap.needsUpdate = true;
  draw();
};
