// SPDX-License-Identifier: MPL-2.0
// Local accents use positions exported by the same geometry that they animate.
export function makeBiomeMotifs(THREE, scene, id, world) {
  if(!['riverside','alpine','marsh','orchard'].includes(id)) return ()=>{};
  const turn=2*Math.PI;
  const group=new THREE.Group();
  scene.add(group);
  const painters=[];
  const drop=new THREE.IcosahedronGeometry(1,0);
  const ring=new THREE.RingGeometry(.91,1,22);
  const facet=new THREE.PlaneGeometry(1,1);
  const seed=i=>{const n=Math.sin(i*127.1+311.7)*43758.5453;return n-Math.floor(n);};
  const envelope=t=>THREE.MathUtils.smoothstep(Math.min(t,1-t),0,.17);

  function particles(n,geometry,palette,additive,paint) {
    for(let i=0;i<n;i++) {
      const material=new THREE.MeshBasicMaterial({
        color:palette[i%palette.length],side:THREE.DoubleSide,transparent:true,depthWrite:false,
        blending:additive?THREE.AdditiveBlending:THREE.NormalBlending,
      });
      const mesh=new THREE.Mesh(geometry,material);
      group.add(mesh);
      const offset=seed(i+1);
      painters.push(phase=>{
        const t=(phase+offset)%1;
        material.opacity=paint(mesh,i,t,phase)*envelope(t);
        mesh.visible=material.opacity>.001;
      });
    }
  }

  if(id==='riverside' && world.wheelContact) {
    const [x,y,z]=world.wheelContact;
    particles(18,drop,[0xd8e9e2,0xf4e2ae,0xb8d7d9],false,(m,i,t)=>{
      const depth=seed(i+22)-.5;
      m.position.set(x+(seed(i+40)-.5)*.75+.39*t,y+.05+Math.sin(Math.PI*t)*(.46+.43*seed(i+8)),z+depth*.43+t*(.46+.36*seed(i+2)));
      const s=.028+.028*seed(i+15);
      m.scale.set(s,s*(1.10+.64*(1-t)),s);
      return .84;
    });
    particles(5,ring,[0xc6dbcf,0xe3dfb8],false,(m,i,t)=>{
      m.position.set(x+(seed(i+5)-.5)*.76,y+.015+i*.001,z+.57+seed(i+18)*.66);
      m.rotation.x=-Math.PI/2;
      const r=.09+t*(.47+.26*seed(i+23));m.scale.set(r,r*.72,1);
      return .36;
    });
  }

  if(id==='alpine') {
    const banks=world.snowOrigins??[];
    if(banks.length) particles(26,drop,[0xe5edf8,0xaeccde,0xf7f5f0],false,(m,i,t,phase)=>{
      const [x,y,z]=banks[i%banks.length],s=.035+.045*seed(i+26);
      m.position.set(x+(seed(i+9)-.5)*1.9+3.0*t,y+.13+.64*Math.sin(t*Math.PI)+.15*seed(i+2),z+.78*t+Math.sin(t*turn+i)*.16);
      m.scale.set(s*(1.8+seed(i)),s*.65,s);
      // Short gusts lift powder at different times along the bank.
      const gust=Math.max(0,Math.cos(turn*phase+(i%banks.length)*.91));
      return .80*gust*gust;
    });
    particles(7,facet,[0xd7eaf1,0xf2e6c7],true,(m,i,t,phase)=>{
      m.position.set(1+7*seed(i+21)+.11*Math.sin(phase*turn+i),.022,-5.7+11*seed(i+17));
      m.rotation.x=-Math.PI/2;m.rotation.z=.14*(seed(i+6)-.5);
      m.scale.set(.22+.75*Math.sin(t*Math.PI),.035+.021*seed(i),1);
      return .18+.12*Math.sin(t*Math.PI);
    });
  }

  if(id==='marsh') {
    const beds=world.glowOrigins??[];
    if(beds.length){
      particles(beds.length*2,drop,[0xd5df91,0xe3c57c],true,(m,i,t,phase)=>{
        const [x,y,z]=beds[i%beds.length],a=phase*turn+i;
        m.position.set(x+.70*Math.sin(a)+.13*Math.sin(a*3),y+.18+.22*Math.cos(a*2),z+.47*Math.cos(a));
        m.scale.setScalar(.043+.035*Math.sin(t*Math.PI));
        return .25+.39*Math.sin(t*Math.PI)**2;
      });
      particles(beds.length,ring,[0xb5bb88],true,(m,i,t)=>{
        const [x,,z]=beds[i];m.position.set(x,.022,z);m.rotation.x=-Math.PI/2;
        const r=.07+.65*t;m.scale.set(r,r*.61,1);
        return .14;
      });
    }
  }

  if(id==='orchard') {
    const crowns=[...(world.leafOrigins??[])].sort((a,b)=>
      (a[0]+2)**2+(a[2]+7)**2-((b[0]+2)**2+(b[2]+7)**2)).slice(0,8);
    const petal=new THREE.Shape();
    petal.moveTo(0,-.50);petal.bezierCurveTo(.47,-.14,.49,.29,.16,.43);
    petal.lineTo(0,.34);petal.lineTo(-.15,.44);petal.bezierCurveTo(-.47,.23,-.42,-.2,0,-.5);
    if(crowns.length) particles(crowns.length*3,new THREE.ShapeGeometry(petal),[0xf4d5c2,0xeedcc2,0xd59755],false,(m,i,t)=>{
      const [x,y,z,ground]=crowns[i%crowns.length],a=t*turn+i;
      m.position.set(x+.95*t+.36*Math.sin(a),THREE.MathUtils.lerp(y-.18,ground+.10,t)+.08*Math.sin(a*2),z+.52*t+.27*Math.cos(a));
      m.rotation.set(a*.67,i+2.5*t,a*.77);
      const s=.12+.14*seed(i+2);m.scale.set(s,s*.82,1);
      return .76;
    });
  }
  return phase=>{for(const paint of painters)paint(phase);};
}
