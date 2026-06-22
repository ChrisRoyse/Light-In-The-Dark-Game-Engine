package render

import (
	"testing"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
)

func TestApplyBakedSunVertexColorsFSV(t *testing.T) {
	geom := geometry.NewBox(1, 1, 1)
	cfg := litasset.DefaultBakedSunConfig()
	t.Logf("FSV baked sun geometry BEFORE items=%d colorVBO=%v defines=%v", geom.Items(), geom.VBO(gls.VertexColor) != nil, geom.ShaderDefines)
	snap, err := ApplyBakedSunVertexColors(geom, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV baked sun geometry AFTER snapshot=%+v colorVBO=%v defines=%v", snap, geom.VBO(gls.VertexColor) != nil, geom.ShaderDefines)
	if !snap.VertexColorBuffer || !snap.ShaderDefine || snap.VertexCount != geom.Items() || !close32(snap.MinScalar, cfg.MinScalar) || !close32(snap.MaxScalar, cfg.MaxScalar) {
		t.Fatalf("baked sun snapshot wrong: %+v cfg=%+v items=%d", snap, cfg, geom.Items())
	}
	if geom.ShaderDefines[VertexColorShaderDefine] != "1" {
		t.Fatalf("missing vertex color shader define: %v", geom.ShaderDefines)
	}
}

func TestApplyBakedSunExistingVertexColorsFSV(t *testing.T) {
	geom := geometry.NewPlane(1, 1)
	colors := math32.ArrayF32{}
	for i := 0; i < geom.Items(); i++ {
		colors.Append(0.5, 0.5, 0.5)
	}
	geom.AddVBO(gls.NewVBO(colors).AddAttrib(gls.VertexColor))
	beforeVBOs := len(geom.VBOs())
	cfg := litasset.BakedSunConfig{Direction: [3]float32{0, 0, 1}, MinScalar: 0.25, MaxScalar: 1}
	snap, err := ApplyBakedSunVertexColors(geom, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV baked sun existing colors BEFORE vbos=%d AFTER vbos=%d snapshot=%+v", beforeVBOs, len(geom.VBOs()), snap)
	if !snap.ExistingColors || len(geom.VBOs()) != beforeVBOs {
		t.Fatalf("existing vertex color VBO should be reused: before=%d after=%d snapshot=%+v", beforeVBOs, len(geom.VBOs()), snap)
	}
	colorVBO := geom.VBO(gls.VertexColor)
	colorVBO.ReadVectors3(gls.VertexColor, func(c math32.Vector3) bool {
		if !close32(c.X, 0.5) || !close32(c.Y, 0.5) || !close32(c.Z, 0.5) {
			t.Fatalf("existing colors not multiplied by scalar 1: %+v", c)
		}
		return true
	})
}

func TestApplyBakedSunVertexColorEdgesFSV(t *testing.T) {
	if _, err := ApplyBakedSunVertexColors(nil, litasset.DefaultBakedSunConfig()); err == nil {
		t.Fatal("nil geometry accepted")
	} else {
		t.Logf("FSV baked sun nil geometry AFTER err=%v", err)
	}

	empty := geometry.NewGeometry()
	if _, err := ApplyBakedSunVertexColors(empty, litasset.DefaultBakedSunConfig()); err == nil {
		t.Fatal("geometry without normals accepted")
	} else {
		t.Logf("FSV baked sun no normals BEFORE items=%d AFTER err=%v", empty.Items(), err)
	}

	badCfg := litasset.DefaultBakedSunConfig()
	badCfg.Direction = [3]float32{}
	geom := geometry.NewBox(1, 1, 1)
	if _, err := ApplyBakedSunVertexColors(geom, badCfg); err == nil {
		t.Fatal("bad baked sun config accepted")
	} else {
		t.Logf("FSV baked sun bad config BEFORE colorVBO=%v AFTER err=%v colorVBO=%v", geom.VBO(gls.VertexColor) != nil, err, geom.VBO(gls.VertexColor) != nil)
	}
}
