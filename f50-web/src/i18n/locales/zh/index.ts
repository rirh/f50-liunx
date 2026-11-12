/**
 * The Chinese messages: the source of every key. One namespace per area; each English file is typed against its
 * Chinese one (`Messages["<namespace>"]`), so the two can never disagree on which keys exist.
 */
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

const zh = {
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

export type Messages = typeof zh;
export default zh;
