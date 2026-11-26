/**
 * A riverside sawmill, built in voxel courses. World units are roughly one metre.
 * The river surface and the sky belong to the host scene.
 */
export function buildWorld(box) {
  let state = 370167;
  const rand = () => ((state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296);
  const pick = palette => palette[Math.floor(rand() * palette.length)];
  const soil = ['#795c40','#705740','#826342','#6e543d'];
  const grass = ['#75834b','#829051','#6d8047','#869154','#74874c','#7b8c4e'];
  const turf = ['#84944f','#8b9657','#74884a','#899754'];
  const stone = ['#707268','#808277','#777d73','#686e68','#858378','#757b70'];
  const cobble = ['#77766a','#8a8676','#827d6e','#6e7065','#99917b'];
  const pine = ['#34523c','#3a5940','#476447','#2b4838','#526d48'];
  const bark = ['#5f4434','#6b5039','#74563a'];
  const firBark = '#503e32';
  const wood = ['#986b43','#a6774a','#a27245','#8f623e','#aa794b'];
  const panel = ['#997547','#a07b4b','#a68050','#907049','#98754c'];
  const timber = '#533c30', beamLit = '#664b35', cutEnd = '#bc925e';
  const roof = ['#465653','#52605a','#3f5151','#53645d','#4b5d56','#58675c','#465854'];
  const lit = '#ffb455';
  const B = box;
  const post = (x,base,z,w,h,col) => B(x,base+h/2,z,w,h,w,col);
  function beam(x1,z1,x2,z2,y,w,h,color) {
    const dx=x2-x1,dz=z2-z1;
    B((x1+x2)/2,y,(z1+z2)/2,w,h,Math.hypot(dx,dz),color,Math.atan2(dx,dz));
  }
  function shore(x) {
    return -.8 + .52*Math.sin(x*.57)+.82*Math.sin(x*.18+.9)
      -3.9*Math.exp(-Math.pow((x-8.3)/2.9,2))
      +2.6*Math.exp(-Math.pow((x+11)/4.6,2));
  }
  function height(x,z) {
    const t=shore(x)-z;
    const back=Math.max(0,-z-9);
    const mound=back*(.14+.09*Math.cos(x*.19))+
      Math.max(0,Math.abs(x+1)-9)*.075*(1-Math.exp(-Math.max(t,0)/4));
    return Math.floor((.79+Math.min(t,1.5)*.26 + mound + .17*Math.sin(x*.52+z*.31))/.43)*.43;
  }
  const step=1.55;
  const isMillrace=(ix,iz)=>ix>=4&&ix<=5&&iz>=-4&&iz<=-1;
  function topAt(x,z) {
    const ix=Math.round(x/step), iz=Math.round(z/step), gx=ix*step, gz=iz*step;
    return gz>shore(gx) || isMillrace(ix,iz) ? -.32 : height(gx,gz);
  }
  // Deep ridges have broad, stepped silhouettes and very little surface detail.
  for(let layer=0;layer<2;layer++) {
    const width=layer===0?4:2.7, n=layer===0?58:66;
    for(let i=-n/2;i<=n/2;i++) {
      const x=i*width,z=layer===0?-57:-37;
      const h=Math.max(3,Math.round((layer===0?10:6.7)+
        (layer===0?4:2)*Math.sin(x*.094+layer)+2.9*Math.cos(x*.18-.8+layer)+
        (layer===0?1.2:1)*Math.sin(x*.36+layer)));
      B(x,h/2-1,z+Math.floor(Math.sin(x*.13)*1.8),width,h+2,layer===0?9:6,
        layer===0?pick(['#777e78','#78817b','#7b827b']):pick(['#5d6e60','#647464','#607364']));
      if(layer===1&&i%2===0) B(x,h-.3,z+1.8,width*.78,1.45,2.7,'#687761');
    }
  }
  // Bank geology, grass caps and occasional exposed stone faces.
  for(let ix=-24;ix<=23;ix++) {
    const x=step*ix, edge=shore(x);
    for(let iz=-22;iz<=3;iz++) {
      const z=step*iz;
      // Leave an open millrace through the inner bank, connected to the river.
      if(z>edge || isMillrace(ix,iz)) continue;
      const h=height(x,z), onEdge=z+step>edge, hNext=topAt(x,z+step);
      B(x,(h-2.25)/2,z,step,h+1.95,step,pick(soil));
      B(x,h-.08,z,step,.16,step,pick(grass));
      if(onEdge || (h-hNext>.65&&rand()<.38)) {
        B(x,h-.29,z+.035,step*.99,.24,step+.025,pick(soil));
        if(rand()<.75) {
          const sw=.38+rand()*.7;
          B(x+(rand()-.5)*.55,h-.68,z+step*.44,sw,.4+rand()*.25,.31,pick(stone));
        }
      }
      if(onEdge&&rand()<.7) {
        const sw=.43+rand()*.85;
        B(x+rand()*.36,h*.12-.12,z+step*.68,sw,.4+rand()*.43,sw*.83,pick(stone));
      }
    }
  }
  function tree(x,z,s,type) {
    const y=topAt(x,z);
    if(y<0)return;
    const broad=type==='broad';
    post(x,y,z,s*(broad?.58:.46),s*(broad?3.45:5),broad?pick(bark):firBark);
    if(broad) {
      const leaf=['#687b43','#728745','#82914a','#5c763f','#71854b','#879448'];
      const levels=[[0,0,3.82,2.75,1.65],[.78,.06,3.69,1.74,1.55],[-.7,.24,4.02,1.8,1.55],[.16,-.62,4.15,2.15,1.62],[-.2,.04,4.99,1.72,.85]];
      for(const [dx,dz,dy,w,h] of levels) B(x+s*dx,y+s*dy,z+s*dz,s*w,s*h,s*(w*.86),pick(leaf));
      for(let i=0;i<6;i++) {
        const a=i*1.047+rand()*.15;
        B(x+s*Math.sin(a)*1.35,y+s*(3.6+rand()*.7),z+s*Math.cos(a)*1.05,.74*s,.69*s,.75*s,pick(leaf));
      }
      return;
    }
    const tiers=[[2.12,2.6],[2.99,2.27],[3.72,1.86],[4.39,1.39],[4.95,.88],[5.4,.4]];
    for(let i=0;i<tiers.length;i++) {
      const [dy,w]=tiers[i],c=pick(pine);
      B(x,y+dy*s,z,w*s,.92*s,w*.68*s,c);
      B(x,y+(dy+.015)*s,z,w*.66*s,.93*s,w*s,c);
      if(i<4) {
        const sign=rand()<.5?-1:1;
        B(x+sign*w*.33*s,y+(dy-.19)*s,z+w*.22*s,w*.37*s,.56*s,w*.45*s,pick(pine));
        B(x-sign*w*.25*s,y+(dy+.152)*s,z-w*.3*s,w*.44*s,.66*s,w*.34*s,pick(pine));
      }
    }
  }
  // The outer tree line has a varied canopy with occasional gaps between crowns.
  for(let row=0;row<3;row++) {
    for(let x=-38;x<39;x+=2.6+rand()*1.8) {
      const z=-29+row*5.15+(rand()-.5)*2.8;
      const s=.81+rand()*.77+(row===0?.17:0);
      tree(x,z,s,rand()<.17?'broad':'fir');
    }
  }
  for(const [x,z,s,t] of [
    [-16,-6,1.38,'fir'],[-12,-5.5,1.56,'fir'],[-9,-8,1.21,'fir'],[-7,-7.4,1.12,'broad'],
    [-13,-1,1.35,'broad'],[-19,-1,1.55,'fir'],[8.6,-11.8,1.13,'fir'],[12,-8.2,1.66,'fir'],
    [16,-5.6,1.18,'broad'],[18,-4,1.53,'fir'],[-19,-9.7,1.52,'fir'],
    [-3.8,-12.4,1.22,'fir'],[1.9,-11.9,1.33,'fir'],[21,-6,1.68,'fir']
  ]) tree(x,z,s,t);

  // Rubble footing, sill beams, plank infill, and a small upper loft.
  const x0=-1.55,x1=5.65,z0=-7.35,z1=-2.28,base=1.46,eave=4.35,cz=(z0+z1)/2,cx=(x0+x1)/2;
  B(cx,.94,cz,x1-x0+.18,1.04,z1-z0+.19,'#69695e');
  B(x1-.13,.21,-4.04,.47,1.03,3.92,'#65675f');
  for(let course=0;course<2;course++) {
    for(let i=0;i<9;i++) B(x0+.4+i*.8,.73+course*.42,z1+.1,.76,.4,.28,pick(cobble));
    for(let i=0;i<6;i++) B(x1+.1,.73+course*.42,z0+.42+i*.825,.28,.4,.79,pick(cobble));
  }
  B(cx,(base+eave)/2,cz,x1-x0,eave-base,z1-z0,pick(panel));
  for(let i=0;i<10;i++) {
    const y=base+.16+i*.285;
    B(cx,y,z1+.015,x1-x0,.268,.085,pick(panel));
    B(x1+.016,y,cz,.086,.268,z1-z0,pick(panel));
  }
  for(const x of [x0,x0+2.02,x0+4.53,x1]) post(x,base-.12,z1+.08,.23,eave-base+.22,timber);
  for(const z of [z0,cz,z1]) post(x1+.07,base-.1,z,.23,eave-base+.18,timber);
  for(const y of [base+.02,eave-.09]) {
    B(cx,y,z1+.1,x1-x0+.29,.24,.29,beamLit);
    B(x1+.1,y,cz,.29,.24,z1-z0+.31,beamLit);
  }
  function frontWindow(x,y,w,h) {
    B(x,y,z1+.115,w+.22,h+.22,.16,timber);
    B(x,y,z1+.211,w,h,.053,lit);
    B(x,y,z1+.255,.083,h,.083,beamLit);
    B(x,y+.03,z1+.257,w,.085,.078,beamLit);
    B(x,y-h/2-.11,z1+.21,w+.48,.15,.4,beamLit);
  }
  frontWindow(-.54,3.1,1.03,1.06);
  frontWindow(4.27,3.05,1.55,1.16);
  // Door with inset panels, a glazed top light and a little iron latch.
  B(1.68,2.52,z1+.14,1.37,2.19,.2,timber);
  for(let i=0;i<5;i++) B(1.19+i*.247,2.48,z1+.255,.228,1.98,.11,pick(wood));
  B(1.69,2.95,z1+.329,.66,.55,.04,lit);
  B(1.69,2.95,z1+.36,.07,.58,.06,timber);
  B(1.69,2.95,z1+.36,.69,.065,.06,timber);
  B(1.69,2.15,z1+.32,1.12,.13,.11,beamLit);
  B(2.03,2.43,z1+.38,.11,.13,.09,'#495051');

  // The roof has a heavy fascia, overlapping courses and a capped ridge.
  const over=.54, run=(z1-z0)/2+over, rows=9, dz=run/rows, rise=.255;
  for(let i=0;i<rows;i++) {
    const y=eave+i*rise,zOff=run-(i+.5)*dz;
    if(i>0) {
      B(cx,y-.12,cz,x1-x0-.04,.38,zOff*2+.09,pick(panel));
      B(x1+.045,y-.14,cz,.135,.235,zOff*2+.16,pick(wood));
    }
    for(const side of [-1,1]) {
      const z=cz+side*zOff;
      B(cx,y+.025,z,x1-x0+1.14,.23,dz+.14,'#423e33');
      const span=x1-x0+1.18;
      let edge=cx-span/2, k=0;
      while(edge<cx+span/2-.005) {
        const tile=Math.min(cx+span/2-edge, k===0&&i%2?.37:.72+.12*rand());
        B(edge+tile/2,y+.164+rand()*.015,z,tile-.012,.155,dz+.11,pick(roof));
        edge+=tile;k++;
      }
    }
  }
  B(cx,eave+rows*rise+.03,cz,x1-x0+1.27,.28,.38,'#626c60');
  B(x1+.155,eave+.83,cz,.17,1.63,.17,beamLit);
  B(x1+.155,eave+.065,cz,.17,.17,z1-z0,beamLit);
  B(x1+.156,4.98,cz-.67,.18,.67,.52,timber);
  B(x1+.262,4.98,cz-.67,.046,.45,.32,lit);

  // Masonry stack rises through the rear pitch, with a projecting crown.
  const chx=.14,chz=-5.41,cht=7.42;
  B(chx,(cht+4.3)/2,chz,.87,cht-4.3,.82,'#7f7566');
  for(let i=0;i<7;i++) {
    const y=5.02+i*.337;
    B(chx,y,chz,.91,.095,.86,'#635e54');
    for(let k=0;k<2;k++) B(chx-.227+k*.444,y+.16,chz+.42,.41,.235,.045,pick(cobble));
  }
  B(chx,cht-.13,chz,1.11,.24,1.02,'#77766a');
  B(chx,cht+.005,chz,.61,.036,.58,'#423d36');

  // Mill wheel. Twin stepped oak rims enclose projecting paddles and oak spokes.
  const wx=x1+.7,wy=1.65,wz=-3.94, r=1.64, pix=.265;
  B(wx,wy,wz,2.02,.34,.34,timber);
  const rotor=(x,y,z,w,h,d,color)=>B(x,y,z,w,h,d,color,0,{pivot:[wx,wy,wz],axis:'x',spin:.19});
  for(const face of [-1,1]) {
    const xx=wx+face*.38;
    for(let j=-6;j<=6;j++)for(let k=-6;k<=6;k++){
      const d=Math.hypot(j,k);
      if(d>5.3&&d<6.42) rotor(xx,wy+k*pix,wz+j*pix,.177,pix+.01,pix+.01,pick(wood));
      else if(d<5.5&&(j===0||k===0||Math.abs(k)===Math.abs(j))) rotor(xx,wy+k*pix,wz+j*pix,.145,pix+.008,pix+.008,beamLit);
    }
    rotor(xx,wy,wz,.23,.57,.57,cutEnd);
  }
  for(let i=0;i<12;i++) {
    const a=i*Math.PI/6;
    rotor(wx,wy+Math.cos(a)*r,wz+Math.sin(a)*r,.99,.17+Math.abs(Math.sin(a))*.14,.17+Math.abs(Math.cos(a))*.14,pick(wood));
  }
  B(wx+.62,wy,wz,.22,.26,.26,'#51534c');
  post(wx+.73,-.55,wz,.24,2.26,timber);
  B(wx+.73,-.60,wz,.66,.22,.68,'#69695e');
  // Rubble lining and a low sill expose the gap beneath the wheel.
  B(5.48,.19,-3.50,.38,.45,4.32,'#77766a');
  B(8.48,.19,-3.50,.43,.45,4.32,'#77766a');
  for (const x of [5.50,8.46]) {
    for (let i=0;i<7;i++) B(x,.45,-5.40+i*.62,.43,.11,.60,'#827d6e');
  }

  // Boardwalk wraps the threshold and carries a narrow pier out over the water.
  function deck(x,z,w,d,y) {
    const n=Math.ceil(w/.32),q=w/n;
    for(let i=0;i<n;i++) B(x-w/2+q*(i+.5),y,z,q-.018,.19,d,pick(wood));
    for(const zz of [z-d*.33,z+d*.33]) B(x,y-.17,zz,w+.07,.18,.19,beamLit);
  }
  deck(2.05,-1.47,7.88,1.34,1.4);
  deck(3.98,-.45,1.62,.76,1.14);
  deck(3.98,.2,1.65,.61,.9);
  deck(3.98,1.85,1.73,2.89,.67);
  deck(3.04,3.34,3.59,1.11,.67);
  for(const [x,z,h] of [[-1.69,-.88,1.95],[.14,-.88,1.97],[2.59,-.88,1.96],[5.83,-.88,1.92],[3.21,1.02,.88],[4.74,1.02,.93],[1.34,3.8,1.01],[4.72,3.8,1.03]]) {
    post(x,-.6,z,.21,h+.6,beamLit);
    B(x,h+.015,z,.215,.035,.215,cutEnd);
  }
  B(.46,1.88,-.88,4.41,.11,.105,beamLit);
  B(.44,1.63,-.88,4.38,.08,.09,wood[0]);
  function lantern(x,y,z,s=1) {
    B(x,y,z,.25*s,.35*s,.25*s,lit);
    for(const sy of [-1,1]) B(x,y+sy*.208*s,z,.34*s,.09*s,.34*s,'#454941');
    for(const dx of [-.124,.124])for(const dz of [-.124,.124])post(x+dx*s,y-.185*s,z+dz*s,.042*s,.37*s,'#46483e');
    B(x,y+.31*s,z,.1*s,.14*s,.1*s,'#494b41');
  }
  B(2.65,3.71,z1+.36,.11,.14,.68,timber);
  lantern(2.65,3.34,z1+.62,.9);
  lantern(4.73,1.32,3.8,1);
  // Log store, chopping block and a small moored flat-bottomed boat.
  for(let i=0;i<3;i++) for(let j=0;j<3-i;j++) {
    const lx=-3.63+j*.56+i*.26, ly=1.48+i*.44;
    B(lx,ly,-3.73,.56,.48,2.05,pick(bark));
    B(lx,ly,-2.695,.46,.39,.045,cutEnd);
    B(lx,ly,-2.663,.2,.16,.025,'#93653f');
  }
  B(-2.48,1.43,-.88,.66,.54,.65,'#795338'); B(-2.48,1.71,-.88,.67,.06,.66,cutEnd);
  function crate(x,y,z,s) {
    B(x,y+s/2,z,s,s,s,pick(wood));
    for(const sy of [.09,.9]) {
      B(x,y+s*sy,z,s*1.045,s*.105,s*1.045,beamLit);
    }
    for(const dx of [-.4,.4]) {
      B(x+s*dx,y+s*.5,z+s*.503,s*.10,s,.06,beamLit);
    }
  }
  crate(5.26,1.5,-1.37,.59);crate(5.76,1.5,-1.83,.48);
  crate(1.9,.765,3.24,.49);
  const bx=2.1,bz=2.12;
  B(bx,.055,bz,.81,.205,2.1,'#785337');
  for(const sx of [-1,1]) B(bx+sx*.424,.232,bz,.122,.27,1.95,pick(wood));
  for(const sz of [-1,1]) {
    B(bx,.229,bz+sz*1.02,.72,.267,.22,pick(wood));
    B(bx,.289,bz+sz*1.16,.46,.21,.19,pick(wood));
  }
  B(bx,.30,bz-.26,.77,.095,.28,cutEnd);
  B(bx-.09,.35,bz+.26,.08,.07,1.12,'#b08754',.36);
  beam(bx+.37,bz-.82,3.21,1.02,.325,.037,.041,'#8b775b');

  // Shore plants grow in clumps and share the same snapped bank heights.
  function tuft(x,y,z,s,colors) {
    for(let i=0;i<3;i++) {
      const h=(.18+rand()*.34)*s;
      B(x+(rand()-.5)*s*.26,y+h/2,z+(rand()-.5)*s*.26,.09*s,h,.1*s,pick(colors));
    }
  }
  for(let i=0;i<395;i++) {
    const x=rand()*55-28,z=shore(x)-rand()*13;
    if(x>-4.9&&x<7.2&&z>-8.8)continue;
    const y=topAt(x,z);
    if(y<0)continue;
    tuft(x,y,z,.56+rand()*.88,turf);
    if(rand()<.086) {
      const h=.26+rand()*.12;
      post(x,y,z,.05,h,'#687341');B(x,y+h+.055,z,.13,.12,.13,pick(['#dfb564','#d3a95b','#c89070','#d6c896']));
    }
  }
  function reeds(x,z,n) {
    for(let i=0;i<n;i++) {
      const xx=x+(rand()-.5)*1.55,zz=z+(rand()-.5)*.94, h=.48+rand()*.77;
      post(xx,-.12,zz,.063,h,pick(['#83854b','#89914e','#777e43']));
      if(rand()<.43)B(xx,h-.13,zz,.094,.23,.095,'#795c3a');
      B(xx+.07,h*.24-.08,zz,.12,h*.57,.07,'#788844');
    }
  }
  reeds(-8.4,1.05,17);reeds(-13.6,2.9,17);reeds(9,-3.27,11);
  // A nearer spit of land at frame right catches the last light.
  for(const [x,z,w,h,d] of [[11.8,5.3,4.2,.59,3.8],[13.7,5.5,3.2,.78,5.3],[15.5,4.8,3.6,.88,6.9],[17.6,4.1,3.1,1.03,8.5],[10.3,5.32,1.55,.39,2.06]]) {
    B(x,(h-.96)/2,z,w,h+.64,d,pick(soil));
    B(x,h-.08,z,w,.16,d,pick(grass));
    for(let i=0;i<8;i++)tuft(x+(rand()-.5)*w,h,z+(rand()-.5)*d,.85,turf);
  }
  reeds(10.05,5.4,12);
  for(const [x,z,w,h] of [[8.9,4.63,.86,.45],[9.6,5.75,.64,.34],[9.28,3.96,.42,.27],[-5.9,1.33,1.07,.44],[-5.17,1.59,.54,.24],[-9,3.2,.72,.24],[-.6,2,.47,.18],[-13.8,4.83,.89,.39],[7,-1.48,.51,.29]]) {
    B(x,h*.32-.06,z,w,h,w*.73,pick(stone));
    B(x-.05,h*.82-.07,z,w*.7,.085,w*.54,pick(cobble));
  }
  return {chimney:[chx,cht+.05,chz],wheelContact:[wx,.04,wz+.45]};
}
