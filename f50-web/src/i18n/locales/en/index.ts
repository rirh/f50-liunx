/** The English messages, one file per namespace, each typed against its Chinese source. */
import app from "./app";
import cellular from "./cellular";
import common from "./common";
import dashboard from "./dashboard";
import maintenance from "./maintenance";
import nav from "./nav";
import network from "./network";
import sms from "./sms";
import system from "./system";
import tools from "./tools";
import type { Messages } from "../zh";

const en: Messages = {
  common,
  app,
  nav,
  dashboard,
  cellular,
  sms,
  network,
  system,
  maintenance,
  tools,
};
export default en;
