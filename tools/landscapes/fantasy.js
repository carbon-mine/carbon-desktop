// Original block sculptures. Dimensions follow the same metre scale as the scene models.
export function addFantasy(box,id) {
  function at(x,y,z,yaw=0,scale=1) {
    const c=Math.cos(yaw),s=Math.sin(yaw);
    return (u,v,w,dx,dy,dz,color,turn=0)=>box(
      x+scale*(u*c+w*s),y+scale*v,z+scale*(-u*s+w*c),
      dx*scale,dy*scale,dz*scale,color,yaw+turn);
  }

  // A distant sky island links the landscapes. Its crown follows the biome.
  const isles={
    farmland:[-42,9.0,-29,.80], riverside:[-34,11.7,-34,1.0],
    alpine:[-39,18.7,-38,.87], marsh:[-27,10.4,-40,.90],
    orchard:[-31,11.7,-37,.70],
  };
  if (!Object.hasOwn(isles,id)) return;
  const [ix,iy,iz,size]=isles[id];
  const island=at(ix,iy,iz,.11,size);
  const turf=id==='alpine'?'#d0dfe3':id==='marsh'?'#627863':'#839957';
  const rock=id==='marsh'?'#675d73':'#756f77';
  island(0,-.32,0,6.1,.64,3.0,rock);
  island(-.76,-1.02,.02,3.92,.76,2.45,'#69656d');
  island(-1.33,-1.75,.11,2.13,.72,1.33,'#716977');
  island(-1.53,-2.38,.12,.88,.55,.81,'#625e6a');
  island(0,.12,0,6.17,.24,3.05,turf);
  island(1.91,.40,-.32,1.63,.32,1.75,turf);
  if(id==='alpine') {
    for(const [x,h,w] of [[-1.3,2.7,.7],[-.45,4.8,.88],[.35,1.9,.57]]){
      island(x,.24+h/2,.12,w,h,w,'#8ebbcf');
      island(x,.24+h+.21,.12,w*.58,.42,w*.58,'#d7edeb');
    }
  }else if(id==='riverside'||id==='marsh'){
    for(const x of [-1.62,.68]){
      island(x,1.60,.13,.68,2.72,.84,'#888784');
      island(x,3.08,.13,1.13,.28,1.04,'#99918f');
    }
    island(-.47,3.42,.13,3.34,.4,1.04,'#899178');
    island(-1.45,3.76,-.02,.79,.28,.67,turf);
  }else{
    island(-.91,1.30,-.04,.46,2.12,.52,'#7d6053');
    const crown=id==='orchard'?'#c59567':'#719067';
    island(-.93,2.67,-.04,2.92,1.10,2.35,crown);
    island(-1.34,3.51,-.12,1.87,.59,1.62,crown);
    island(.30,2.47,.28,1.13,.73,1.26,crown);
  }
  if(id==='farmland') {
    // A harvest ward watches from the grass strip between barley and the yard.
    const b=at(-6.8,0,1.92,.37);
    b(0,.83,0,.14,1.68,.15,'#775138');
    b(0,1.23,0,1.37,.13,.13,'#91633d');
    b(0,1.19,.016,.41,.61,.30,'#425c67');
    b(-.18,.94,.04,.17,.22,.22,'#5c747a');
    b(.13,.94,.04,.14,.31,.22,'#384e5b');
    for(const sign of [-1,1]){
      b(sign*.39,1.25,0,.39,.227,.22,'#53717a');
      b(sign*.56,1.20,.012,.10,.09,.15,'#d0a95d');
    }
    b(0,1.65,0,.46,.38,.42,'#d17a31');
    b(0,1.65,.21,.27,.27,.034,'#de903d');
    for(const u of [-.085,.085])b(u,1.685,.231,.055,.073,.024,'#553b2c');
    b(.014,1.59,.231,.127,.04,.024,'#72472c');
    b(0,1.86,-.018,.68,.082,.60,'#ae8748',-.04);
    b(0,1.96,-.034,.42,.13,.36,'#a17a43');
    b(.018,2.055,-.045,.31,.062,.30,'#c29a54');
    b(-.12,1.45,.184,.32,.095,.055,'#b7513a');
    b(-.235,1.31,.19,.103,.25,.052,'#bb5d42');
  }

  if(id==='riverside') {
    // An old river gauge on the nearer grass spit. The inset channels meet an amber lens.
    const b=at(11.25,.59,5.20,.38);
    b(0,.115,0,.88,.23,.73,'#696c60');
    b(-.015,.34,-.02,.65,.22,.55,'#838575');
    b(0,1.00,0,.50,1.1,.43,'#737e75');
    b(-.04,1.67,0,.41,.24,.42,'#92978a');
    b(-.07,1.85,0,.29,.12,.35,'#a1a294');
    const face=.222;
    for(const [x,y,w,h] of [[0,.70,.055,.30],[.076,.865,.19,.046],[.145,.972,.045,.20],[.055,1.08,.20,.048],[0,1.16,.05,.15]]){
      b(x,y,face,w,h,.026,'#90b6a0');
    }
    b(0,1.33,face,.19,.185,.049,'#574f3f');
    b(0,1.33,face+.034,.113,.111,.033,'#ffb455');
    for(const [x,y,z,w] of [[-.20,.225,.19,.24],[.13,.444,.15,.19],[-.13,1.905,.04,.12]]){
      b(x,y+.013,z,w,.031,w*.72,'#798948');
    }
  }

  if(id==='alpine') {
    // Prismatic ice grows out of the existing snow-capped skerries.
    const spire=(x,y,z,s,yaw)=>{
      const b=at(x,y,z,yaw,s);
      b(0,.50,0,.23,1.0,.25,'#77b6c7');
      b(.145,.59,0,.070,.79,.19,'#96ced7');
      b(-.13,.43,-.012,.049,.78,.16,'#638fae');
      b(.006,1.065,-.01,.17,.13,.17,'#bbdfea');
      b(.014,1.16,-.015,.087,.07,.081,'#d6eff2');
    };
    for(const [x,z,rockSize,crystal] of [[-9.1,6.3,.82,1.10],[-11.2,-16.44,1.15,.94],[10.14,-21.56,.73,.67]]){
      const top=.342*rockSize;
      spire(x+.12*rockSize,top-.025,z-.09*rockSize,crystal,.29);
      spire(x-.22*rockSize,.304*rockSize-.022,z+.24*rockSize,crystal*.55,-.18);
      spire(x+.25*rockSize,.304*rockSize-.028,z+.25*rockSize,crystal*.35,.68);
    }
  }

  if(id==='marsh') {
    // Floating moon lilies sit outside the landing and the open punt channel.
    for(const [x,z,s,yaw] of [[-1.85,5.7,1.08,.39],[-1.03,6.60,.68,-.47],[-4.2,-2,.79,.28],[1.05,-11,.64,-.65]]){
      const b=at(x,.004,z,yaw,s);
      b(-.12,.019,0,.55,.039,.67,'#476e64');
      b(.247,.020,-.097,.18,.040,.48,'#5b8771');
      b(-.035,.023,.34,.45,.042,.085,'#669174');
      b(0,.088,0,.070,.099,.073,'#698569');
      for(let i=0;i<6;i++){
        const a=i*Math.PI/3;
        b(Math.sin(a)*.119,.158+(i%2)*.018,Math.cos(a)*.119,.108,.103,.170,'#c392bc',a);
        b(Math.sin(a)*.165,.222+(i%2)*.018,Math.cos(a)*.165,.090,.052,.092,'#f0bfdb',a);
      }
      b(0,.204,0,.115,.132,.11,'#eebc81');
      b(0,.289,0,.073,.040,.07,'#ffb455');
    }
  }

  if(id==='orchard') {
    // A pair of bee skeps with a copper leaf finial, just off the harvest yard.
    for(const [x,z,s,yaw] of [[6.98,5.95,1.06,-.10],[7.95,5.44,.85,-.21]]){
      const b=at(x,0,z,yaw,s);
      for(const u of [-.24,.24])for(const v of [-.22,.22])b(u,.22,v,.085,.44,.10,'#795b42');
      b(0,.47,0,.78,.095,.75,'#977149');
      for(const [y,w,col] of [[.61,.60,'#a87938'],[.79,.65,'#d09a4c'],[.97,.59,'#bf853b'],[1.15,.48,'#d7a750'],[1.31,.32,'#c99543']]){
        b(0,y,0,w,.177,w*.92,col);
      }
      b(0,.644,.283,.21,.087,.027,'#4c402f');
      b(0,.574,.341,.24,.035,.175,'#ae803f');
      b(0,1.46,0,.046,.135,.05,'#686046');
      b(.069,1.51,0,.14,.068,.091,'#6b8965',.18);
    }
  }
}

