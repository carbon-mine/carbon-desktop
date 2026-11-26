// SPDX-License-Identifier: MPL-2.0
// Wind strips a sparse trail of husks from three points inside the wheat plot.
export function makeFarmChaff(THREE, scene) {
  const geometry=new THREE.BufferGeometry();
  geometry.setAttribute('position',new THREE.Float32BufferAttribute([
    -.5,0,0, .14,.20,0, .52,.04,0, -.5,0,0, .52,.04,0, .04,-.12,0,
  ],3));
  geometry.setAttribute('color',new THREE.Float32BufferAttribute([
    .68,.43,.17, 1,.81,.44, 1,.88,.56, .68,.43,.17, 1,.88,.56, .84,.61,.26,
  ],3));
  const material=new THREE.MeshBasicMaterial({vertexColors:true,side:THREE.DoubleSide});
  const origins=[[-16.6,-7.2],[-13.4,-3.9],[-11.8,.9]];
  const n=21, mesh=new THREE.InstancedMesh(geometry,material,n);
  mesh.frustumCulled=false;scene.add(mesh);
  const transform=new THREE.Object3D();
  return phase=>{
    for(let i=0;i<n;i++){
      const origin=origins[i%origins.length];
      const age=(phase+i*.138+.17*(i%3))%1;
      const span=.24+.05*(i%4), flutter=2*Math.PI*age;
      transform.position.set(
        origin[0]+(i%4)*.29+age*(1.7+.26*(i%3)),
        .59+Math.sin(Math.PI*age)*(.63+.16*(i%4)),
        origin[1]+(i%5)*.21+age*.78+.13*Math.sin(flutter+i)
      );
      const size=span*Math.sin(Math.PI*age)**2;
      transform.scale.setScalar(size);
      transform.rotation.set(.45*Math.sin(flutter*2+i),flutter+i*.8,flutter*1.5+i);
      transform.updateMatrix();mesh.setMatrixAt(i,transform.matrix);
    }
    mesh.instanceMatrix.needsUpdate=true;
  };
}
