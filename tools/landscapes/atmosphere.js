// SPDX-License-Identifier: MPL-2.0
import * as THREE from 'three';

const turn = Math.PI * 2;
const clamp = THREE.MathUtils.clamp;
const opacityEnvelope = age => Math.sin(Math.PI * age) ** 2;

// Paint a continuous bank of vapour into one alpha layer. Individual transparent
// boxes expose their internal faces when seen from above.
function cloudTexture(bank, dusk) {
  const canvas=document.createElement('canvas');
  canvas.width=160; canvas.height=80;
  const ctx=canvas.getContext('2d');
  const image=ctx.createImageData(canvas.width,canvas.height);
  const lobes=[[-.58,.12,.36,.35],[-.27,-.05,.42,.46],[.08,-.17,.39,.50],[.42,.06,.37,.32],[.64,.16,.22,.21]];
  for(let y=0;y<canvas.height;y++){
    for(let x=0;x<canvas.width;x++){
      const u=(x+.5)/canvas.width*2-1, v=(y+.5)/canvas.height*2-1;
      const w=bank ? -u : u;
      const warp=.034*Math.sin(w*18+v*8)+.022*Math.sin(w*31-v*13)+.010*Math.sin(w*45+v*17);
      const density=Math.max(...lobes.map(([cx,cy,rx,ry]) =>
        1-((w-cx)/rx)**2-((v+warp-cy)/ry)**2));
      const edge=clamp(density*2.7,0,1);
      const alpha=edge*edge*(3-2*edge);
      const light=clamp(.64-.44*v+.09*Math.sin(w*12+v*5),0,1);
      const color=dusk ? [179,170,188] : [218,219,211];
      const at=(y*canvas.width+x)*4;
      for(let c=0;c<3;c++) image.data[at+c]=color[c]+Math.round(light*(252-color[c]));
      image.data[at+3]=Math.round(245*alpha);
    }
  }
  ctx.putImageData(image,0,0);
  const texture=new THREE.CanvasTexture(canvas);
  texture.colorSpace=THREE.SRGBColorSpace;
  return texture;
}

// Position the nearby breeze in a shallow plane in front of the camera. Its
// geometry occupies the outskirts of both landscape and overhead compositions.
export function makeBreeze(scene, camera, overhead, dusk, direction) {
  const layer = new THREE.Group();
  camera.add(layer);
  scene.add(camera);
  const cloudGroups = [];
  const box = new THREE.BoxGeometry();
  for (let bank = 0; bank < 2; bank++) {
    const material = new THREE.SpriteMaterial({
      map:cloudTexture(bank,dusk), transparent:true, depthWrite:false, toneMapped:false,
    });
    const group=new THREE.Sprite(material);
    layer.add(group); cloudGroups.push({group,material});
  }

  const leaf = new THREE.Shape();
  leaf.moveTo(-.53,0); leaf.lineTo(-.27,.20); leaf.lineTo(.10,.26);
  leaf.lineTo(.56,.035); leaf.lineTo(.19,-.19); leaf.lineTo(-.18,-.17); leaf.closePath();
  const leafMaterial = new THREE.MeshBasicMaterial({color:dusk ? 0x987a64 : 0xa48540, transparent:true, side:THREE.DoubleSide, depthWrite:false});
  const blade = new THREE.Mesh(new THREE.ShapeGeometry(leaf),leafMaterial);
  const twig = new THREE.Group();
  const twigMaterial = new THREE.MeshBasicMaterial({color:dusk ? 0x775e55 : 0x806244, transparent:true, depthWrite:false});
  const stalk = new THREE.Mesh(box,twigMaterial); stalk.scale.set(.84,.055,.055); twig.add(stalk);
  const spur = new THREE.Mesh(box,twigMaterial); spur.scale.set(.35,.045,.045); spur.rotation.z=.57; spur.position.set(-.06,.09,0);twig.add(spur);
  layer.add(blade,twig);

  // A broad, low-contrast atmospheric wash on the sunward side. It never
  // crosses the field of view or flashes as the camera crosses the loop seam.
  const material = new THREE.ShaderMaterial({
    transparent:true,depthWrite:false,depthTest:false,
    uniforms:{amount:{value:0},sunSide:{value:direction}},
    vertexShader:'varying vec2 uvp;void main(){uvp=uv;gl_Position=vec4(position.xy*2.,.99,1.);}',
    fragmentShader:`varying vec2 uvp;uniform float amount;uniform float sunSide;
      void main(){
        vec2 source=vec2(sunSide<0.?-.10:1.10,1.13);
        float glow=exp(-dot((uvp-source)*vec2(1.8,1.35),(uvp-source)*vec2(1.8,1.35))*4.);
        gl_FragColor=vec4(1.,.88,.67,amount*glow);
        #include <colorspace_fragment>
      }`
  });
  const glare = new THREE.Mesh(new THREE.PlaneGeometry(),material);
  glare.frustumCulled=false;glare.renderOrder=20;layer.add(glare);

  return phase => {
    const distance=8;
    const halfH=distance*Math.tan(THREE.MathUtils.degToRad(camera.fov)/2);
    const halfW=halfH*camera.aspect;
    for (const [i,{group,material:cloud}] of cloudGroups.entries()) {
      const age=(phase+i*.5+.18)%1;
      const x=(i===0 ? -.84 : .63)+direction*(age-.5)*.40;
      const y=overhead ? (i===0 ? .72 : -.68) : (i===0 ? .77 : .88);
      group.position.set(x*halfW,y*halfH,-distance);
      const width=halfW*(overhead ? .63 : .42);
      group.scale.set(width,width*(overhead ? .39 : .22),1);
      cloud.opacity=opacityEnvelope(age)*(overhead ? .39 : .37);
    }
    for(const [i,particle] of [blade,twig].entries()){
      const age=((phase+i*.49+.048)%1)/.37;
      particle.visible=age<1;
      if(!particle.visible) continue;
      particle.position.set(
        direction*(age*2.45-1.22)*halfW,
        ((i===0 ? .54 : -.39)-age*.21+Math.sin(age*turn*1.2+i)*.045)*halfH,
        -distance
      );
      particle.scale.setScalar(halfW*(i===0 ? .029 : .021));
      particle.rotation.set(Math.sin(age*turn+i)*.62,Math.sin(age*turn*1.5)*.68,-.35+age*2.9);
      const fade=clamp(Math.min(age,1-age)*10,0,1);
      (i===0 ? leafMaterial : twigMaterial).opacity=.80*fade;
    }
    material.uniforms.amount.value=(overhead ? .087 : dusk ? .13 : .20)*(.90+.10*Math.sin(turn*phase));
  };
}
