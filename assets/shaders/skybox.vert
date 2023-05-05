//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout (location = 0) out vec3 outViewDir;

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	mat4 invProjView;
	vec4 cameraPos;
	vec4 ambientLight;
	vec3 directionalLightColour;
	float directionalLightStrength;
	vec4 directionalLightDirection;
	mat4 sunProj;
	mat4 sunView;
	int pointLightCount;
	int padding[3];
} cameraData;

layout( push_constant ) uniform constants
{
	int handle;
} pushConstants;

void main()
{
	vec3 pos = vPosition;
	mat4 invproj = inverse(cameraData.proj);
	pos.xy	  *= vec2(invproj[0][0],invproj[1][1]);
	pos.z 	= -1.0f;

	outViewDir	= transpose(mat3(cameraData.view)) * normalize(pos);
	gl_Position	= vec4(vPosition, 1.0).xyww;
}