//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/camera.glsl"

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout (location = 0) out vec3 outViewDir;

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