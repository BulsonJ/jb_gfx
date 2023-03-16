//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout(std140,set = 0, binding = 0) uniform  CameraBuffer{
	mat4 view_proj;
} cameraData;

//output variable to the fragment shader
layout (location = 0) out vec3 outColor;

void main()
{
	outColor = vec3(1.0,1.0,1.0);
	gl_Position = vec4(1.0, 0.0, 0.0,0.0);
}