//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout(std140,set = 0, binding = 0) uniform  CameraBuffer{
	mat4 view_proj;
} cameraData;

struct QuadDrawData {
	mat4 transform;
	vec3 colour;
	uint textureIndex;
	uint textIndex;
	uint padding;
	uint padding2;
	uint padding3;
};

layout(std140,set = 0, binding = 1) readonly buffer QuadDrawDataBuffer{
	QuadDrawData data[];
} quadData;

struct TextDrawData {
	vec2 vertex1;
	vec2 vertex2;
	vec2 vertex3;
	vec2 vertex4;
	vec2 texCoord1;
	vec2 texCoord2;
	vec2 texCoord3;
	vec2 texCoord4;
};

layout(std140,set = 0, binding = 2) readonly buffer TextDrawDataBuffer{
	TextDrawData data[];
} textData;

//output variable to the fragment shader
layout (location = 0) out vec3 outColor;
layout (location = 1) out vec2 outTexCoords;
layout (location = 2) out uint outTexID;

void main()
{
//const array of positions for the triangle
	const vec3 positions[4] = vec3[4](
		vec3(0.f,0.f, 0.0f),
		vec3(1.f,0.f, 0.0f),
		vec3(0.f,1.f, 0.0f),
		vec3(1.f,1.f, 0.0f)
	);

	const vec2 texCoords[4] = vec2[4](
    	vec2(0.f,0.f),
    	vec2(1.f,0.f),
    	vec2(0.f,1.f),
    	vec2(1.f,1.f)
    );

    int transformIndex = gl_VertexIndex / 6;
    int vertexIndex = gl_VertexIndex % 6;
	QuadDrawData quadDraw = quadData.data[nonuniformEXT(transformIndex)];
	if ( quadDraw.textIndex > 0) {
		TextDrawData textDraw = textData.data[nonuniformEXT(quadDraw.textIndex - 1)];

		const vec2 textPositions[4] = vec2[4](
			textDraw.vertex1,
		textDraw.vertex2,
		textDraw.vertex3,
		textDraw.vertex4
		);

		const vec2 textTexCoords[4] = vec2[4](
		textDraw.texCoord1,
		textDraw.texCoord2,
		textDraw.texCoord3,
		textDraw.texCoord4
		);

		gl_Position = cameraData.view_proj * vec4(textPositions[vertexIndex], 0.0f, 1.0f);
		outTexCoords = textTexCoords[vertexIndex];
	} else {
		gl_Position = cameraData.view_proj * quadDraw.transform * vec4(positions[vertexIndex], 1.0f);
		outTexCoords = texCoords[vertexIndex];
	}
	outColor = quadDraw.colour;
	outTexID = quadDraw.textureIndex;
}