#version 460
#extension GL_EXT_nonuniform_qualifier: enable

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) flat in uint inTexID;

//output write
layout (location = 0) out vec4 outFragColor;

layout (set = 0, binding = 3) uniform sampler2D bindlessTextures[];

void main()
{
	vec3 outColour = inColor;
	if (inTexID > 0){
	    vec4 texture = texture(bindlessTextures[nonuniformEXT(inTexID - 1)], inTexCoords);
	    if (texture.a == 0) {
	        discard;
	    } else {
	        outColour = texture.rgb * outColour;
	    }
	}
	outFragColor = vec4(outColour,1.0f);
}