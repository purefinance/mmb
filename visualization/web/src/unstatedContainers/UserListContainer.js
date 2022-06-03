import { Container } from "unstated";
import CryptolpAxios from "../cryptolpaxios";

class UserListContainer extends Container {
  state = { users: [], roles: [], loading: false };

  async loadUserList() {
    await this.setState({ loading: true });
    const role = await CryptolpAxios.getRoles();
    const user = await CryptolpAxios.getUsers("");

    await this.setState({
      roles: role,
      users: user,
      loading: false,
    });
  }

  updateUserRole = (userId, value) => {
    let userIdx = this.state.users.findIndex((u) => u.id === userId);

    let usersBefore = this.state.users.slice(0, userIdx);
    let usersAfter = this.state.users.slice(
      userIdx + 1,
      this.state.users.length
    );

    let user = this.state.users[userIdx];
    user.role = value;

    let users = [...usersBefore, user, ...usersAfter];
    this.setState({ users: users });
  };

  saveUser = (user) => CryptolpAxios.updateUser(user);
}

export default UserListContainer;
