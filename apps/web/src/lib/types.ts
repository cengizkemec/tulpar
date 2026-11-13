import type { components } from "@tulpar/shared-types";

export type Schemas = components["schemas"];

export type SelfUser = Schemas["SelfUserDto"];
export type Community = Schemas["CommunityDto"];
export type Channel = Schemas["ChannelDto"];
export type Role = Schemas["RoleDto"];
export type Message = Schemas["MessageDto"];
export type SyncResponse = Schemas["SyncResponse"];
export type SyncCommunity = Schemas["SyncCommunityDto"];
export type ReadState = Schemas["ReadStateDto"];
export type VoiceParticipant = Schemas["VoiceParticipantDto"];
export type VoiceToken = Schemas["VoiceTokenDto"];
export type LoginResponse = Schemas["LoginResponse"];
export type GatewayTicket = Schemas["GatewayTicketDto"];
export type Friend = Schemas["FriendUserDto"];
export type Friendship = Schemas["FriendshipDto"];
export type BlockedUser = Schemas["BlockedUserDto"];
export type DirectConversation = Schemas["DirectConversationDto"];
