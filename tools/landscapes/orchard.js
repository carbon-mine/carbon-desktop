/** A working hillside cider orchard, from the harvest floor to the distant downland. */
export function buildOrchard(box) {
  let seed = 1172649;
  const rand = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
  const range = (lo, hi) => lo + (hi - lo) * rand();
  const pick = a => a[Math.floor(rand() * a.length)];
  const leafOrigins = [];
  const grass = ['#9b9c4b','#9d9e50','#a1a154','#999b4d','#a2a255'];
  const herbs = ['#8d9649','#a5a84d','#b1a84e','#959b47','#b6a355'];
  const soil = ['#ba9364','#be9666','#bf9869','#ba9566'];
  const rock = ['#8b8377','#a29a85','#99917f','#918979','#aba08a','#847b6e'];
  const wood = ['#986b43','#a6774a','#a27245','#8f623e','#aa794b'];
  const bark = ['#67503b','#594335','#775844','#6d503b'];
  const apples = ['#ae3022','#c83c23','#bc3821','#dc5931','#c74529','#d3502a'];
  const leaves = [
    ['#8f943b','#9f9d3e','#b0aa44','#abb04e','#bcb44d','#ccd05b'],
    ['#ab862e','#bd9431','#cea239','#dbb444','#ebba46','#efca5d'],
    ['#ae6d2f','#c08032','#d39137','#e19c3d','#e8a84a','#f1b751'],
    ['#9b5c2c','#ad6e30','#c07931','#cc8537','#d58f40','#df9d44'],
    ['#8e8835','#a09b38','#b3a83d','#c2b344','#d0bc4c','#dfca5b'],
  ];
  function local(x,y,z,angle=0) {
    const c = Math.cos(angle), s = Math.sin(angle);
    return (u,v,w,sw,sh,sd,color,yaw=0) =>
      box(x+u*c+w*s,y+v,z-u*s+w*c,sw,sh,sd,color,angle+yaw);
  }

  // Each column contains contiguous benches. The road occupies separate surface
  // intervals and can follow each terrace without a second coplanar ground skin.
  const step = 1.4, firstX = -53.2;
  const centerX = x => firstX + step * Math.round((x-firstX)/step);
  const contour = x => 1.8*Math.sin((x+3)*.083)+.003*(x+2)*(x+2);
  const ledges = [-4.25,-11.9,-19.3,-26.9,-34.7,-43.0];
  const levels = [0,.88,1.88,3.05,4.28,5.4,6.14];
  const edge = (i,x) => ledges[i]+contour(x)*(1-.064*i);
  function ground(x,z) {
    for (let i=0;i<ledges.length;i++) if(z>edge(i,centerX(x))) return levels[i];
    return levels.at(-1);
  }
  const lane = z => 2.0+3.0*Math.sin((z+5)*.104);
  const isDirt = (x,z) => Math.abs(x-lane(z))<1.15 ||
    ((x-.7)/6.4)**2+((z-1.45)/4.5)**2<1;
  for(let col=0;col<=76;col++) {
    const x = firstX+step*col;
    for(let tier=0;tier<levels.length;tier++) {
      const near=tier===0?27:edge(tier-1,x);
      const far=tier===ledges.length?-56:edge(tier,x);
      const top=levels[tier];
      const landColor=pick(grass);
      let start=far, dirt=isDirt(x,far+.01);
      const emit=(end) => {
        if(end-start<.001)return;
        box(x,(top-1.35)/2,(start+end)/2,step,top+1.35,end-start,dirt?pick(soil):landColor);
      };
      for(let z=far+.50;z<near-.01;z+=.50){
        const next=isDirt(x,z);
        if(next!==dirt){emit(z);start=z;dirt=next;}
      }
      emit(near);
    }
  }

  // Low stone revetments follow the terraces. Openings line up with the uphill lane.
  for(let row=0;row<5;row++) {
    for(let c=9;c<66;c++) {
      const x=firstX+c*step, z=edge(row,x);
      const height=levels[row+1]-levels[row];
      if(Math.abs(x-lane(z))<1.85) continue;
      for(let course=0;course<3;course++){
        const cut=range(.33,.69)*step, chunks=course%2===0?[cut,step-cut]:[step-cut,cut];
        let start=x-step/2;
        for(const width of chunks){
          const h=height/3-.015, offset=.17-course*.024+range(0,.045);
          box(start+width/2,levels[row]+height*(course+.5)/3,z+offset,width-.022,h,.39+range(0,.06),pick(rock));
          start+=width;
        }
      }
      if(rand()<.23) box(x+range(-.25,.25),levels[row+1]+.022,z-.014,range(.32,.72),.044,.34,pick(herbs));
    }
    const cx=lane(ledges[row]+contour(1.5)), cz=edge(row,centerX(cx));
    for(let n=0;n<5;n++){
      const height=(n+1)*(levels[row+1]-levels[row])/5;
      box(cx,levels[row]+height/2,cz+1.62-n*.334,2.25,height,.334,pick(rock));
    }
  }

  // Distant ridges run past the full camera frustum. Three contour lines are
  // drawn at successively lower contrast, with irregular hedgerows on the nearest.
  for(let k=0;k<3;k++){
    const width=2.65, z=[-60,-75,-91][k];
    for(let i=-43;i<=43;i++){
      const x=i*width;
      const top=7.5+k*2.25+Math.sin(x*.052+k*1.9)*(2.2+k*.4)+
        Math.sin(x*.104+.84*k)*1.32+.5*Math.cos(x*.179+k*.7);
      const color=pick([['#8c924e','#939859','#939050','#9d9958'],
        ['#929669','#959970','#9da075'],['#a7a591','#aba995','#a2a38c']][k]);
      box(x,(top-1.35)/2,z,width,top+1.35,[8,22,24][k],color);
      if(k===0 && Math.sin(x*.16)>.05) {
        for(let j=0;j<2;j++){
          const h=range(.53,1.48);
          box(x+range(-.9,.9),top+h*.5,z+range(-1,2),range(.84,1.7),h,range(.68,1.18),pick(['#6d7950','#788254','#7e8653']));
        }
      }
    }
  }

  // Shaggy, spreading apple trees. Lateral boughs stair-step up into foliage
  // so the trunk remains legible underneath the cropped, open-centred canopy.
  function tree(x,z,scale,paletteIndex,detail) {
    const y=ground(x,z), rot=rand()*6.28, S=scale, draw=local(x,y,z,rot);
    const crown=leaves[paletteIndex], trunk=pick(bark);
    const t=(a,b,c,w,h,d,color,turn=0)=>draw(a*S,b*S,c*S,w*S,h*S,d*S,color,turn);
    t(0,.55,0,.42,1.1,.47,trunk,.03);
    t(.045,1.29,-.023,.34,.44,.36,pick(bark),-.14);
    for(let j=0;j<4;j++){
      const a=j*Math.PI/2+range(-.28,.28), dx=Math.cos(a), dz=Math.sin(a);
      if(detail>0) t(dx*.21,.14+j*.008,dz*.21,.40,.22,.33,pick(bark),-a);
      const length=range(.84,1.14), endHeight=range(2.0,2.19);
      for(let p=0;p<3;p++){
        const f=(p+.5)/3;
        t(dx*length*f,1.38+j*.007+(endHeight-1.38)*f,dz*length*f,.28-p*.039,.40,.50,pick(bark),Math.atan2(dx,dz));
      }
      const clusters=detail===0?2:3;
      for(let v=0;v<clusters;v++){
        const lateral=(v-.5)*.46, r=length+(v===2?-.31:.14);
        const ux=dx*r-dz*lateral, uz=dz*r+dx*lateral;
        const cy=endHeight+.27+range(-.07,.22)+(v===2?.44:0);
        const w=range(.92,1.24), h=range(.65,.9), d=range(.89,1.22);
        const turn=range(-.18,.18);
        t(ux,cy,uz,w,h,d,crown[Math.floor(range(0,4))],turn);
        if(detail>0) {
          t(ux+dx*.08,cy+h*.47,uz+dz*.08,w*.75,h*.40,d*.79,crown[Math.floor(range(2,6))],turn+.065);
          t(ux+dx*w*.37,cy-.075,uz+dz*d*.37,w*.51,h*.59,d*.53,crown[Math.floor(range(1,5))],turn+.29);
        }
        if(detail>0 && v<2) {
          const fruitCount=detail===2?3:2;
          for(let n=0;n<fruitCount;n++){
            const slide=(n-(fruitCount-1)/2)*.24+range(-.085,.085);
            const fr=range(.125,.198), fx=ux+dx*.54*w-dz*slide, fz=uz+dz*.54*d+dx*slide;
            const fy=cy-h*.48+range(-.14,.015);
            t(fx,fy,fz,fr,fr*.96,fr*.91,pick(apples),range(-.35,.35));
            if(detail===2) t(fx+.007,fy+fr*.51,fz,fr*.18,fr*.27,fr*.17,'#665135');
          }
        }
      }
    }
    t(.09,2.81,-.05,1.13,.86,1.18,crown[3],.14);
    t(.18,3.21,-.08,.69,.33,.76,crown[5],-.12);
    if(detail===2) leafOrigins.push([x,y+2.78*S,z,y]);
  }

  for(let row=0;row<6;row++){
    for(let col=-5;col<=5;col++){
      const x=col*6.45+(row%2?2.0:-.75)+range(-.37,.37);
      const z=edge(row,x)-3.05+range(-.30,.30);
      if(Math.abs(x-lane(z))<3.3) continue;
      const near=row<2&&Math.abs(x)<20;
      tree(x,z,range(.86,1.17)*(row===5?.92:1),
        (Math.abs(col*3+row*2)+Math.floor(rand()*3))%leaves.length,
        near?2:row<4&&Math.abs(x)<29?1:0);
    }
  }
  tree(-6.0,5.25,1.50,0,2);
  tree(9.98,4.92,1.54,2,2);
  tree(-11.1,.35,1.32,3,2);
  tree(15,-2.0,1.25,4,2);

  function crate(draw,x,y,z,w=1.0,h=.61,d=.76,full=true) {
    const wall=.085, post=.097;
    draw(x,y+.075,z,w,.15,d,'#87644b');
    for(const sx of [-1,1]) for(const sz of [-1,1])
      draw(x+sx*(w-post)/2,y+h*.50,z+sz*(d-post)/2,post,h,post,'#87644b');
    for(let r=0;r<3;r++){
      const yy=y+.17+r*(h-.19)/2;
      for(const sz of [-1,1]) draw(x,yy,z+sz*(d-wall)/2,w-post*2,.100,wall,pick(wood));
      for(const sx of [-1,1]) draw(x+sx*(w-wall)/2,yy,z,wall,.100,d-post*2,pick(wood));
    }
    if(!full)return;
    draw(x,y+h-.19,z,w-.17,.14,d-.17,'#94442c');
    for(let i=0;i<4;i++)for(let j=0;j<3;j++){
      const ax=x+(i-1.5)*(w-.19)/4, az=z+(j-1)*(d-.20)/3, size=range(.155,.202);
      draw(ax,y+h-.091+range(0,.034),az,size,size*.88,size,pick(apples),range(-.55,.55));
    }
  }

  // Freestanding basket press: stone pads, mortised timber frame, drained
  // bed, open-slat basket with iron bands, follower and threaded hand screw.
  const press=local(-.78,0,1.58,.17);
  const darkIron='#464744', metal='#686963';
  for(const u of [-1.07,1.07]){
    press(u,.12,0,.56,.24,1.52,pick(rock));
    press(u,.33,0,.47,.18,1.45,'#77563d');
    press(u,1.88,0,.33,2.93,.42,'#87644b');
    press(u,.62,0,.38,.086,.46,darkIron);
  }
  for(const z of [-.60,.60]) press(0,.61,z,2.61,.29,.24,'#664b35');
  press(0,.79,0,2.52,.15,1.49,'#937456');
  for(const x of [-.99,.99]) press(x,.905,0,.107,.10,1.47,'#937456');
  press(0,3.28,0,2.74,.34,.48,'#77563d');
  for(const x of [-1.07,1.07]){
    press(x,3.28,.252,.115,.12,.025,metal);
    press(x,3.466,0,.20,.041,.27,'#533c30');
  }
  const radius=.71, count=16, staves=.249;
  for(let k=0;k<count;k++){
    const a=k*Math.PI*2/count, sx=Math.sin(a), sz=Math.cos(a);
    press(sx*radius,1.39,sz*radius,staves,.98,.105,pick(wood),a);
    for(const yy of [1.09,1.68])press(sx*(radius+.062),yy,sz*(radius+.062),.308,.097,.066,darkIron,a);
  }
  // Different plank lengths fit the follower to the round basket.
  for(let i=-2;i<=2;i++){
    const x=i*.242, len=2*Math.sqrt(.65*.65-x*x);
    press(x,1.823,0,.229,.12,len,'#bc925e');
  }
  press(0,1.98,0,.85,.20,.25,'#87644b');
  press(0,2.73,0,.144,1.45,.144,darkIron);
  for(let n=0;n<12;n++) press(0,2.10+n*.086,0,.191,.032,.165,metal,n*.66);
  press(0,3.505,0,.34,.12,.33,darkIron);
  press(0,3.678,0,.165,.25,.17,metal);
  press(0,3.77,0,1.70,.10,.10,'#664b35',-.16);
  for(const s of [-1,1])press(s*.784,3.77,s*.126,.25,.16,.18,'#a6774a',-.16);
  press(.29,.787,.96,.24,.09,.57,'#77563d');
  for(const side of [-1,1])press(.29+side*.125,.837,.96,.038,.086,.57,'#937456');
  function tub(draw,x,z,r,h){
    const n=12;
    for(let j=0;j<n;j++){
      const a=j*Math.PI*2/n, dx=Math.sin(a), dz=Math.cos(a);
      draw(x+dx*r,h*.50,z+dz*r,r*.49,h,.064,pick(wood),a);
      for(const y of [h*.19,h*.82])draw(x+dx*(r+.035),y,z+dz*(r+.035),r*.56,.045,.035,darkIron,a);
    }
    draw(x,h*.20,z,r*1.49,.12,r*1.49,'#9d592b',.14);
  }
  tub(press,.29,1.31,.30,.47);
  const stacked=local(-2.66,0,2.33,.07);
  crate(stacked,0,0,0,1.02,.66,.82,false);
  crate(stacked,.033,.688,-.007,.98,.52,.79,true);
  crate(stacked,-.49,0,.93,.98,.58,.74,true);
  crate(local(-.24,0,-.44,-.39),0,0,0,1.02,.56,.76,true);

  // Two load-bearing wheels, a through axle and a parking prop carry this
  // little handcart. The raised handles leave its open tail accessible.
  const cart=local(4.00,0,.86,-.55);
  const rad=.635;
  for(const side of [-1,1]){
    const x=side*.924;
    for(let iy=-6;iy<=6;iy++)for(let iz=-6;iz<=6;iz++){
      const q=Math.hypot(iy,iz);
      if(q<=5.29||q>6.49)continue;
      cart(x,rad+iy*.097,iz*.097,.149,.097,.097,'#574c41');
    }
    cart(x,rad,0,.104,1.10,.077,'#9c7955');
    cart(x,rad+.003,0,.112,.087,1.10,'#9c7955');
    for(const sy of [-1,1])for(const sz of [-1,1])for(let n=1;n<=4;n++)
      cart(x,rad+sy*n*.084,sz*n*.084,.098+n*.002,.125,.125,'#8d694b');
    cart(x+side*.031,rad,0,.208,.234,.234,'#77563d');
    cart(x+side*.145,rad,0,.045,.104,.104,metal);
  }
  cart(0,rad,0,1.94,.157,.157,darkIron);
  for(const x of [-.55,.55]){
    cart(x,.785,-.03,.16,.22,2.42,'#77563d');
    cart(x,.83,1.61,.118,.12,1.55,'#87644b');
    cart(x,.86,2.39,.135,.15,.29,'#bc925e');
  }
  cart(.46,.38,.85,.11,.76,.12,'#77563d');
  for(let j=0;j<6;j++)cart((j-2.5)*.250,.944,-.04,.239,.134,2.25,pick(wood));
  for(const side of [-1,1]){
    for(const z of [-1.02,.02,1.00]) cart(side*.749,1.22,z,.11,.66,.12,'#87644b');
    for(const y of [1.11,1.41]) cart(side*.790,y,-.01,.073,.158,2.21,pick(wood));
  }
  for(const y of [1.11,1.41])cart(0,y,-1.076,1.51,.156,.077,'#a6774a');
  crate(cart,0,1.014,-.53,1.33,.53,.84,true);
  crate(cart,.018,1.014,.42,1.27,.48,.80,true);

  // A low picking ladder under the left-hand apple tree; rungs sit between
  // the rails while successively offset rail blocks give it a real lean.
  const ladder=local(-4.90,ground(-4.90,6.08),6.08,.955);
  for(let i=0;i<8;i++){
    const yy=i*.316+.168, zz=-i*.215, half=.47-i*.013;
    for(const side of [-1,1])ladder(side*half,yy,zz,.115,.342,.238,'#87644b');
    ladder(0,yy+.055,zz,2*half-.11,.085,.093,'#bc925e');
  }
  crate(local(-4.19,0,6.52,-.21),0,0,0,.82,.49,.64,true);

  // Ground detail gathers in patches at the trees, then thins out across the lane.
  for(let i=0;i<1440;i++){
    const x=range(-33,34), z=range(-39,19), y=ground(x,z), dirt=isDirt(x,z);
    if(dirt && (rand()<.73 || ((x+1)/4)**2+((z-1.5)/2.7)**2<1 || Math.hypot(x-4,z-.86)<2.2))continue;
    if(dirt) {
      const s=range(.045,.11);
      box(x,y+s*.21,z,s*1.65,s*.42,s,rand()<.55?'#a68a67':'#b79c75',rand()*3);
    } else {
      const h=range(.08,.23), w=range(.028,.055);
      for(let j=0;j<3;j++) {
        const hh=h*range(.66,1.41);
        box(x+range(-.066,.066),y+hh*.5,z+range(-.065,.065),w,hh,w*.91,pick(herbs),range(0,2));
      }
      if(rand()<.10)box(x,y+h*.26,z+.06,.094,.026,.069,pick(['#d6aa49','#c4893c','#c99b47']),rand()*3);
    }
  }
  for(const [x,z,r] of [[-6,5.25,1.8],[9.98,4.92,1.85],[-3.5,2.9,.8],[4.2,-1.2,.9]]){
    for(let i=0;i<11;i++){
      const xx=x+range(-r,r),zz=z+range(-r,r),s=range(.11,.17), y=ground(xx,zz);
      box(xx,y+s*.44,zz,s,s*.88,s*.96,pick(apples),rand()*2);
    }
  }
  return {chimney:null,leafOrigins};
}
