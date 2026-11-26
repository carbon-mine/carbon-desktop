// SPDX-License-Identifier: MPL-2.0
import * as THREE from 'three';

const turn = Math.PI * 2;
// Locally drawn silhouettes: long ledges, off-centre coves and a few islands.
const decks = [
  [
    '............####.............',
    '.........########............',
    '....######..#####.....###....',
    '...#######...########.####...',
    '..######.....#############...',
    '..#####....########..##......',
    '...##########..#####.........',
    '.......######....#######.....',
    '........####......####.......',
  ],
  [
    '.......#####................',
    '.....########....###........',
    '....#####.###########.......',
    '.######...########..####....',
    '.####.....#######....#####..',
    '..####..##########..#####...',
    '....########......###.......',
    '....#####...................',
  ],
];

// Build just the outside of the connected grid. The top and underside stay
// level, the enclosed gaps remain open and adjacent cells have no inner faces.
function deckGeometry(rows) {
  const positions=[], tones=[];
  const occupied=(x,z)=>rows[z]?.[x]==='#';
  function face(points,tone) {
    for(const i of [0,1,2,0,2,3]) {
      positions.push(...points[i]);
      tones.push(tone,tone,tone);
    }
  }
  for(let z=0;z<rows.length;z++) for(let x=0;x<rows[z].length;x++){
    if(!occupied(x,z)) continue;
    const a=x,b=x+1,c=z,d=z+1,lo=0,hi=.50;
    face([[a,hi,c],[a,hi,d],[b,hi,d],[b,hi,c]],1.0);
    face([[a,lo,c],[b,lo,c],[b,lo,d],[a,lo,d]],.76);
    if(!occupied(x-1,z)) face([[a,lo,c],[a,lo,d],[a,hi,d],[a,hi,c]],.82);
    if(!occupied(x+1,z)) face([[b,lo,d],[b,lo,c],[b,hi,c],[b,hi,d]],.92);
    if(!occupied(x,z-1)) face([[b,lo,c],[a,lo,c],[a,hi,c],[b,hi,c]],.88);
    if(!occupied(x,z+1)) face([[a,lo,d],[b,lo,d],[b,hi,d],[a,hi,d]],.85);
  }
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
  geometry.setAttribute('color',new THREE.Float32BufferAttribute(tones,3));
  return geometry;
}

function makeDeck(scene, camera, overhead, dusk, seed) {
  const formations=[
    {shape:0, x:-.96, y:overhead ? .77 : .75, distance:overhead ? 17 : 42, width:overhead ? .72 : .88, yaw:.38, offset:.04},
    {shape:1, x:.84, y:overhead ? -.84 : .94, distance:overhead ? 20 : 60, width:overhead ? .76 : .94, yaw:-.34, offset:.48},
    {shape:1, x:overhead ? 1.06 : .34, y:overhead ? .88 : .66, distance:overhead ? 24 : 75, width:.41, yaw:.65, offset:.76},
  ];
  const strips=formations.map((spec,i)=>{
    const rows=decks[spec.shape];
    const material=new THREE.MeshBasicMaterial({
      color:dusk ? (i===1 ? 0xbcb3c2 : 0xd2c9cf) : (i===1 ? 0xe5dfd3 : 0xf4eee0),
      vertexColors:true, transparent:true,
    });
    const mesh=new THREE.Mesh(deckGeometry(rows),material);
    // Each cloud has a different plan and position. A stable world-space wind
    // translates it slowly. Its vapour thins before re-forming upstream.
    scene.add(mesh);
    return {mesh, material, spec, columns:rows[0].length};
  });
  const east=new THREE.Vector3(.94,0,.34).normalize();
  let anchors;
  function anchorStrips(){
    const forward=camera.getWorldDirection(new THREE.Vector3());
    const right=new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld,0);
    const up=new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld,1);
    return strips.map(({mesh,spec,columns})=>{
      const halfH=spec.distance*Math.tan(THREE.MathUtils.degToRad(camera.fov)/2);
      const halfW=halfH*camera.aspect;
      const center=camera.position.clone().addScaledVector(forward,spec.distance)
        .addScaledVector(up,spec.y*halfH).addScaledVector(right,spec.x*halfW);
      const cell=halfW*spec.width/columns;
      mesh.rotation.y=Math.atan2(-east.z,east.x)+spec.yaw;
      mesh.scale.setScalar(cell);
      const middle=new THREE.Vector3(columns*.5,0,4).multiplyScalar(cell).applyEuler(mesh.rotation);
      return {base:center.sub(middle),travel:halfW*(overhead ? .31 : .22)};
    });
  }
  return {strips,paint(phase){
    anchors??=anchorStrips();
    for(const [i,{mesh,material,spec}] of strips.entries()){
      const age=(phase+seed*.13+spec.offset)%1;
      const {base,travel}=anchors[i];
      mesh.position.copy(base).addScaledVector(east,(age-.5)*travel);
      const blend=THREE.MathUtils.smoothstep(Math.min(age,1-age),0,.20);
      material.opacity=blend*(overhead ? .76 : .94);
      mesh.visible=blend>.002;
    }
  }};
}

function polygon(points,color){
  const shape=new THREE.Shape(points.map(([x,y])=>new THREE.Vector2(x,y)));
  return new THREE.Mesh(new THREE.ShapeGeometry(shape),new THREE.MeshBasicMaterial({
    color,side:THREE.DoubleSide,transparent:true,depthWrite:false,
  }));
}
function makeDebris(id){
  const palettes={
    alpine:[0x8c7757,0xb3a67c],farmland:[0xb48c45,0xd9bc73],riverside:[0x8e854d,0xb5ac69],
    marsh:[0x9b744c,0xbba376],orchard:[0xa05c2f,0xd28a49],
  };
  const colors=Object.hasOwn(palettes,id) ? palettes[id] : [0x8a7850,0xc9b987];
  const blade=new THREE.Group();
  blade.add(polygon([[-.5,-.03],[-.23,.20],[.05,.24],[.54,.035]],colors[1]));
  blade.add(polygon([[-.5,-.03],[.54,.035],[.21,-.17],[-.17,-.18]],colors[0]));
  const twig=new THREE.Group();
  twig.add(polygon([[-.55,-.02],[.48,.015],[.51,.07],[-.53,.04]],0x77553e));
  twig.add(polygon([[-.08,.03],[.23,.28],[.25,.25],[.02,.025]],0x906d42));
  const tip=polygon([[.20,.23],[.25,.35],[.42,.38],[.36,.27]],colors[1]);twig.add(tip);
  const seed=polygon([[-.40,-.015],[-.18,.12],[.25,.10],[.42,.015],[.18,-.095]],colors[1]);
  return [blade,twig,seed];
}

// Foreground debris is attached to a shallow camera plane. The cloud deck is
// world geometry, so its edge and underside change naturally with perspective.
export function makeBreeze(scene,camera,overhead,dusk,sun,id) {
  const layer=new THREE.Group();
  camera.add(layer);scene.add(camera);
  const seed=[...id].reduce((s,c)=>s+c.charCodeAt(0),0)%17/17;
  const deck=makeDeck(scene,camera,overhead,dusk,seed);
  const debris=makeDebris(id);layer.add(...debris);
  const glareMaterial=new THREE.ShaderMaterial({
    transparent:true,depthWrite:false,depthTest:false,
    uniforms:{amount:{value:0},lightPoint:{value:new THREE.Vector2()}},
    vertexShader:'varying vec2 uvp;void main(){uvp=uv;gl_Position=vec4(position.xy*2.,.99,1.);}',
    fragmentShader:`varying vec2 uvp;uniform float amount;uniform vec2 lightPoint;
      void main(){
        vec2 p=(uvp-lightPoint)*vec2(1.60,1.15);
        float halo=.68*exp(-dot(p,p)*5.)+.32*exp(-dot(p*vec2(.65,2.),p*vec2(.65,2.))*9.);
        gl_FragColor=vec4(1.,.84,.59,amount*halo);
        #include <colorspace_fragment>
      }`,
  });
  const glare=new THREE.Mesh(new THREE.PlaneGeometry(),glareMaterial);
  glare.frustumCulled=false;glare.renderOrder=20;layer.add(glare);
  const sunDir=new THREE.Vector3(...sun).normalize(), right=new THREE.Vector3();
  return phase=>{
    deck.paint(phase);
    const distance=8,halfH=distance*Math.tan(THREE.MathUtils.degToRad(camera.fov)/2),halfW=halfH*camera.aspect;
    right.setFromMatrixColumn(camera.matrixWorld,0);
    const solarSide=sunDir.dot(right)<0 ? -.065 : 1.065;
    glareMaterial.uniforms.lightPoint.value.set(solarSide,overhead ? 1.20 : 1.065);
    glareMaterial.uniforms.amount.value=(overhead ? .17 : dusk ? .20 : .28)*(.91+.09*Math.sin(turn*phase+seed));
    for(const [i,particle] of debris.entries()){
      const age=((phase+.07+seed*.17+[0,.48,.76][i])%1)/[.41,.34,.29][i];
      particle.visible=age<1;
      if(!particle.visible)continue;
      const wave=turn*age;
      const x=2.50*age-1.24;
      const y=[.48,-.41,.75][i]-.23*age+.10*Math.sin(wave*1.2+i+seed);
      particle.position.set(x*halfW,y*halfH,-distance);
      particle.scale.setScalar(halfW*[.052,.047,.035][i]);
      particle.rotation.set(.53*Math.sin(wave+i),.68*Math.sin(wave*1.5+i),-.35+2.55*age);
      const fade=THREE.MathUtils.clamp(Math.min(age,1-age)*12,0,1);
      particle.traverse(obj=>{if(obj.material) obj.material.opacity=.87*fade;});
    }
  };
}
